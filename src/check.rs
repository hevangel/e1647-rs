//! Minimal well-formedness checks over the parsed declarations.
//!
//! This is **not** full symbol/type checking: there is no type system, no
//! cross-file scope, and no elaboration yet. Checks, all single-file scope:
//!
//! - `E0201` two base declarations (`struct`/`unit`/`type`) share a name
//! - `E0202` a declaration declares the same field name twice
//! - `E0203` a field type, a method parameter/return/local-variable type,
//!   or an `extend`/`like` target names a type not declared in this file
//!   (built-in units such as `sys` are accepted; other external
//!   declarations are a later milestone)
//! - `E0204` a `keep` constraint in a `struct`/`unit` references an
//!   identifier that is neither one of its fields nor a declared enumeral
//!   (`me`/`self` are accepted; base fields of `extend` targets are unknown
//!   here, so `extend` declarations are skipped)
//! - `E0205` a `uint`/`int` width literal is zero
//! - `E0206` duplicate name in an inner scope: method parameter, local
//!   variable, or enumeral
//! - `E0207` a subtype derived from `longint`/`longuint` — a
//!   `(bits: N)` width specification on either long base, or a range
//!   qualification (`type t: longint [0..5]`) over it (clause 5.2.3.1)
//! - `E0208` a `typeof()`/`typeof_item()` type position (clause 5.9)
//!   whose argument's static type cannot be determined against the
//!   current minimal type information (a field path, a local `var`, a
//!   method parameter or a literal)
//! - `E0209` `typeof_item()` of a type that is not a list (clause 5.9)
//! - `E0211` a field init value on a generated field (clause 6.9.1: an
//!   init value requires `!`, `const`, or — in the later static-members
//!   stage — `static`)
//! - `E0213` the `!` and `~` field prefixes combined (clause 6.9.1: a
//!   field is either do-not-generate or range-generated, never both)
//! - `E0214` a range-generated (`~`) field declared `const` (clause
//!   6.9.4: a const field is initialised once; a `~` field's reduced set
//!   is produced per generation)
//! - `E0124` `extend` of an interface (clause 6.13: only the declaration
//!   form exists; interfaces are not extensible here)
//! - `E0125` a non-method member inside an `interface` body (clause
//!   6.13: the members are method signatures)
//! - `E0219` an interface inheriting conflicting signatures for one
//!   method from two of its `like` bases (clause 6.13)
//! - `E0311` a struct implementing two interfaces that declare one
//!   method with different signatures (clause 6.3, 31.4) — reported once
//!   per (type, method); the check runs here rather than in `elab.rs`
//!   because the fixture contract observes the well-formedness pass
//! - `E0312` a `new S` value assigned to an interface-typed target the
//!   struct does not implement (clause 5.4.4.4: the automatic cast
//!   requires conformance) — checked for the `var x: I = new S;` and
//!   `x = new S;` shapes, the ones whose static types this pass knows
//!
//! `E0212` ("a `static const` field requires an init value", clause
//! 6.9.4) is reserved for the static-members stage, which owns the
//! contextual `static` prefix; static-prefixed members are rejected
//! earlier, by the parser.
//!
//! Fields, methods and constraints are collected through `when` subtype
//! members as well. A `keep` constraint sees every field of the enclosing
//! declaration, including fields of sibling `when` blocks — subtype-aware
//! scoping is elaboration and a later milestone.

use crate::ast::{
    Declaration, DeclarationKind, Expr, Field, KeepConstraint, Member, MethodDecl, ScalarBase,
    Stmt, TypeExpr,
};
use crate::diagnostic::Diagnostic;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

pub fn check_declarations(declarations: &[Declaration]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    // Single-file scope: base `struct`/`unit`/`type` declarations are the
    // visible type universe. Multiple `extend` blocks of the same target are
    // legal, so duplicates are checked only among base declarations.
    let mut declared: BTreeSet<&str> = BTreeSet::new();
    for declaration in declarations {
        if declaration.kind == DeclarationKind::Extend {
            continue;
        }
        if !declared.insert(declaration.name.as_str()) {
            diagnostics.push(Diagnostic::new(
                "E0201",
                format!("duplicate declaration '{}'", declaration.name),
                declaration.name_span.0,
                declaration.name_span.1,
            ));
        }
    }
    // The interface tables (clause 6.13, 6.3): which names are
    // interfaces, and the effective implemented-interface set per
    // struct/unit type name (like-chain bases and extend blocks
    // included). The E0312/E0311 checks and the statement walk's
    // interface rules read these.
    let interfaces: BTreeMap<&str, &Declaration> = declarations
        .iter()
        .filter(|declaration| declaration.kind == DeclarationKind::Interface)
        .map(|declaration| (declaration.name.as_str(), declaration))
        .collect();
    let struct_impls = struct_interface_sets(declarations);
    let iface = IfaceCtx {
        interfaces: &interfaces,
        struct_impls: &struct_impls,
    };
    let mut reported_implement_conflicts: BTreeSet<(String, String)> = BTreeSet::new();
    check_implement_targets(declarations, &interfaces, &mut diagnostics);
    check_implement_conflicts(
        declarations,
        &interfaces,
        &struct_impls,
        &mut reported_implement_conflicts,
        &mut diagnostics,
    );
    // Enumerals live in the file-global namespace once their `type` is
    // declared, so any `keep` constraint may reference them. Inline
    // enumerals (`kind : [imm, reg]`) contribute the same way.
    let mut enumerals: BTreeSet<&str> = declarations
        .iter()
        .filter(|declaration| declaration.kind == DeclarationKind::Type)
        .flat_map(|declaration| declaration.members.iter())
        .filter_map(|member| match member {
            Member::Enumeral(enumeral) => Some(enumeral.name.as_str()),
            _ => None,
        })
        .collect();
    for declaration in declarations {
        for_each_field(&declaration.members, &mut |field| {
            if let crate::ast::TypeExpr::InlineEnum { names, .. } = &field.ty {
                for name in names {
                    enumerals.insert(name.as_str());
                }
            }
        });
    }
    // The `typeof()` resolution scope (5.9): every declaration's fields
    // by name, so a member path (`typeof(x.n)`) resolves through the
    // named type's field table. `when`-declared fields contribute too.
    let struct_fields: BTreeMap<&str, Vec<(&str, &TypeExpr)>> = declarations
        .iter()
        .map(|declaration| {
            let mut fields = Vec::new();
            for_each_field(&declaration.members, &mut |field| {
                fields.push((field.name.as_str(), &field.ty));
            });
            (declaration.name.as_str(), fields)
        })
        .collect();
    for declaration in declarations {
        check_extend_target(declaration, &declared, &interfaces, &mut diagnostics);
        check_like_target(declaration, &declared, &interfaces, &mut diagnostics);
        if declaration.kind == DeclarationKind::Interface {
            check_interface_members(declaration, &mut diagnostics);
            check_interface_bases(declaration, &interfaces, &mut diagnostics);
            check_interface_inherited_conflicts(declaration, &interfaces, &mut diagnostics);
        }
        if let Some(underlying) = &declaration.underlying {
            check_named_type(underlying, &declared, &mut diagnostics);
            check_long_subtype(
                underlying,
                declaration.subtype_range.is_some(),
                &mut diagnostics,
            );
        }
        check_fields(declaration, &declared, &struct_fields, &mut diagnostics);
        check_enumerals(declaration, &mut diagnostics);
        check_methods(
            declaration,
            &declared,
            &struct_fields,
            &iface,
            &mut diagnostics,
        );
        for_each_on(&declaration.members, &mut |on| {
            let mut seen: BTreeSet<&str> = BTreeSet::new();
            let mut own_fields: BTreeMap<&str, &TypeExpr> = BTreeMap::new();
            for_each_field(&declaration.members, &mut |field| {
                own_fields.insert(field.name.as_str(), &field.ty);
            });
            let mut locals: BTreeMap<&str, &TypeExpr> = own_fields.clone();
            let scope = StatementScope {
                declared: &declared,
                struct_fields: &struct_fields,
                own_fields: &own_fields,
                iface: &iface,
            };
            check_statements(
                &on.body,
                &mut seen,
                &scope,
                &mut locals,
                &on.path.join("."),
                &mut diagnostics,
            );
            // 11.3: "The on action block shall not contain any
            // time-consuming actions or TCMs" — wait/sync/start/compute
            // at any depth of the block are rejected.
            check_on_body_time_consumption(&on.body, &on.path.join("."), &mut diagnostics);
        });
        if declaration.kind != DeclarationKind::Extend {
            let mut field_names: BTreeSet<&str> = BTreeSet::new();
            for_each_field(&declaration.members, &mut |field| {
                field_names.insert(field.name.as_str());
            });
            for_each_constraint(&declaration.members, &mut |constraint| {
                check_constraint_fields(constraint, &field_names, &enumerals, &mut diagnostics);
            });
        }
    }
    diagnostics
}

fn check_extend_target(
    declaration: &Declaration,
    declared: &BTreeSet<&str>,
    interfaces: &BTreeMap<&str, &Declaration>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if declaration.kind != DeclarationKind::Extend {
        return;
    }
    // The parser's pseudo-declaration holding file-level `define`/`import`
    // members is scope, not an extend of a real type.
    if declaration.name == "global" && declaration.like.is_none() {
        return;
    }
    // Clause 6.13: only the interface declaration form exists here — an
    // interface has no body to extend and no `is also` composition; the
    // checker rejects `extend I` outright.
    if interfaces.contains_key(declaration.name.as_str()) {
        diagnostics.push(Diagnostic::new(
            "E0124",
            "extending an interface is not supported (clause 6.13)",
            declaration.name_span.0,
            declaration.name_span.1,
        ));
        return;
    }
    if !declared.contains(declaration.name.as_str())
        && !crate::builtins::is_builtin_unit(declaration.name.as_str())
    {
        diagnostics.push(Diagnostic::new(
            "E0203",
            format!(
                "cannot extend '{}': no struct or unit with this name is declared in this file",
                declaration.name
            ),
            declaration.name_span.0,
            declaration.name_span.1,
        ));
    }
}

fn check_like_target(
    declaration: &Declaration,
    declared: &BTreeSet<&str>,
    interfaces: &BTreeMap<&str, &Declaration>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(like) = &declaration.like else {
        return;
    };
    let like_span = declaration.like_span.unwrap_or(declaration.name_span);
    if interfaces.contains_key(like.as_str()) {
        // A struct/unit cannot `like` an interface (clause 6.3: `like`
        // bases are structs and units; interfaces attach through
        // `implementing`, clause 6.13/31.4). Interfaces never enter the
        // struct composition tables, so the clause would silently
        // contribute nothing — reported instead of ignored.
        diagnostics.push(Diagnostic::new(
            "E0203",
            format!(
                "'{like}' is an interface and cannot be a 'like' base (clause 6.13: use 'implementing')"
            ),
            like_span.0,
            like_span.1,
        ));
    } else if !declared.contains(like.as_str()) && !crate::builtins::is_builtin_base(like) {
        diagnostics.push(Diagnostic::new(
            "E0203",
            format!("unknown type '{like}' in like clause"),
            like_span.0,
            like_span.1,
        ));
    }
}

/// Clause 6.13: an interface body's members are method signatures — no
/// bodies, no fields, constraints, events or subtypes. A signature parses
/// as a body-less method; every other member is `E0125` at its own span.
fn check_interface_members(declaration: &Declaration, diagnostics: &mut Vec<Diagnostic>) {
    for member in &declaration.members {
        if matches!(member, Member::Method(_)) {
            continue;
        }
        let span = interface_member_span(member);
        diagnostics.push(Diagnostic::new(
            "E0125",
            "interface member must be a method signature (clause 6.13)",
            span.0,
            span.1,
        ));
    }
}

/// The span a non-method interface member is diagnosed at (E0125): the
/// member's name or keyword span.
fn interface_member_span(member: &Member) -> crate::ast::Span {
    match member {
        Member::Field(field) => field.name_span,
        Member::Constraint(constraint) => constraint.span,
        Member::When(when) => when.span,
        Member::Event(event) => event.span,
        Member::On(on) => on.span,
        Member::Cover(cover) => cover.span,
        Member::Expect(expect) => expect.span,
        Member::Port(port) => port.span,
        Member::Import { span, .. } | Member::Define { span, .. } => *span,
        Member::Macro(macro_decl) => macro_decl.span,
        Member::Enumeral(enumeral) => enumeral.name_span,
        Member::Method(_) => unreachable!("the caller filters method members"),
    }
}

/// Every `like` base of an interface must itself be an interface (clause
/// 6.13). The declaration name is already known (E0201 dedupes); the base
/// is what can be wrong here.
fn check_interface_bases(
    declaration: &Declaration,
    interfaces: &BTreeMap<&str, &Declaration>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (base, base_span) in &declaration.bases {
        if !interfaces.contains_key(base.as_str()) {
            diagnostics.push(Diagnostic::new(
                "E0203",
                format!("unknown interface '{base}' in like clause"),
                base_span.0,
                base_span.1,
            ));
        }
    }
}

/// The comparable form of a method signature (clause 6.13): the parameter
/// types and the return type, rendered canonically. Parameter names are
/// not part of the signature — `f(x: int)` and `f(y: int)` are the same;
/// an untyped parameter is its own kind and differs from a typed one.
type MethodSignature = (Vec<String>, Option<String>);

/// One method name's signatures across every interface that provides it,
/// paired with each declaring interface's name.
type SignatureSources<'d> = BTreeMap<String, Vec<(&'d str, MethodSignature)>>;

fn method_signature(method: &MethodDecl) -> MethodSignature {
    (
        method
            .params
            .iter()
            .map(|param| match &param.ty {
                Some(ty) => ty.render(),
                None => String::new(),
            })
            .collect(),
        method.return_type.as_ref().map(|ty| ty.render()),
    )
}

/// Every method signature `name` provides (clause 6.13), with the
/// interface that declares it: the transitive `like` bases' signatures
/// first (recursively, in base order), then the interface's own. A
/// repeated method name collects one entry per declaring interface;
/// cycles stop the walk (the elaborator reports them as E0301).
fn interface_signature_sources<'d>(
    name: &'d str,
    interfaces: &BTreeMap<&'d str, &'d Declaration>,
    stack: &mut Vec<&'d str>,
    out: &mut SignatureSources<'d>,
) {
    if stack.contains(&name) {
        return;
    }
    let Some(declaration) = interfaces.get(name) else {
        return;
    };
    stack.push(name);
    for (base, _) in &declaration.bases {
        interface_signature_sources(base.as_str(), interfaces, stack, out);
    }
    for member in &declaration.members {
        let Member::Method(method) = member else {
            continue;
        };
        out.entry(method.name.clone())
            .or_default()
            .push((name, method_signature(method)));
    }
    stack.pop();
}

/// The first pair of distinct interfaces that declare `sources`' method
/// with different signatures.
fn conflicting_signature_sources<'s>(
    sources: &'s [(&'s str, MethodSignature)],
) -> Option<(&'s str, &'s str)> {
    for i in 0..sources.len() {
        for j in i + 1..sources.len() {
            if sources[i].1 != sources[j].1 {
                return Some((sources[i].0, sources[j].0));
            }
        }
    }
    None
}

/// Clause 6.13: an interface includes all of its `like` bases' members;
/// two of them declaring one method with different signatures is a
/// conflict — the composed signature set is ambiguous. Reported once per
/// (interface, method), at the interface's own name.
fn check_interface_inherited_conflicts(
    declaration: &Declaration,
    interfaces: &BTreeMap<&str, &Declaration>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if declaration.bases.is_empty() {
        return;
    }
    let mut sources: SignatureSources = BTreeMap::new();
    let mut stack = Vec::new();
    for (base, _) in &declaration.bases {
        interface_signature_sources(base.as_str(), interfaces, &mut stack, &mut sources);
    }
    for (method_name, entries) in &sources {
        if conflicting_signature_sources(entries).is_some() {
            diagnostics.push(Diagnostic::new(
                "E0219",
                format!(
                    "interface '{}' inherits conflicting signatures for '{method_name}' (clause 6.13)",
                    declaration.name
                ),
                declaration.name_span.0,
                declaration.name_span.1,
            ));
        }
    }
}

/// The effective implemented-interface set per struct/unit type name
/// (clause 6.3): like-chain bases' sets first, then the declaration's
/// own `implementing`, then each non-template extend block's, deduped in
/// written order. Order-independent on declarations (an extend may
/// precede its target): the sets only ever grow, so the scan iterates to
/// a fixpoint.
fn struct_interface_sets(declarations: &[Declaration]) -> BTreeMap<&str, Vec<String>> {
    let mut sets: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut changed = true;
    while changed {
        changed = false;
        for declaration in declarations {
            let contributes = match declaration.kind {
                DeclarationKind::Struct | DeclarationKind::Unit => true,
                DeclarationKind::Extend => !declaration.template_extend,
                DeclarationKind::Type | DeclarationKind::Interface => false,
            };
            if !contributes {
                continue;
            }
            let like_set: Vec<String> = match (&declaration.like, declaration.kind) {
                (Some(base), DeclarationKind::Struct | DeclarationKind::Unit) => {
                    sets.get(base.as_str()).cloned().unwrap_or_default()
                }
                _ => Vec::new(),
            };
            let target = sets.entry(declaration.name.as_str()).or_default();
            let before = target.len();
            for name in like_set
                .into_iter()
                .chain(declaration.implements.iter().cloned())
            {
                if !target.contains(&name) {
                    target.push(name);
                }
            }
            changed |= target.len() != before;
        }
    }
    sets
}

/// Every name in an `implementing` clause must declare an interface
/// (clause 6.3/31.4). A name that is not an interface would silently
/// contribute nothing to the composed set — reported instead of ignored.
fn check_implement_targets(
    declarations: &[Declaration],
    interfaces: &BTreeMap<&str, &Declaration>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for declaration in declarations {
        if declaration.implements.is_empty() {
            continue;
        }
        for interface_name in &declaration.implements {
            if !interfaces.contains_key(interface_name.as_str()) {
                diagnostics.push(Diagnostic::new(
                    "E0203",
                    format!("unknown interface '{interface_name}' in implementing clause"),
                    declaration.name_span.0,
                    declaration.name_span.1,
                ));
            }
        }
    }
}

/// Clause 6.3/31.4: a struct implementing two interfaces that declare one
/// method with different signatures is a conflict — no single signature
/// satisfies both. Checked over the EFFECTIVE interface set (the
/// declaration's own `implementing` plus what its like-chain bases and
/// extend blocks contribute), once per (type, method). Reported at the
/// contributing declaration's name — the fixture contract observes this
/// pass, so the check lives here rather than in `elab.rs`'s composition.
fn check_implement_conflicts(
    declarations: &[Declaration],
    interfaces: &BTreeMap<&str, &Declaration>,
    struct_impls: &BTreeMap<&str, Vec<String>>,
    reported: &mut BTreeSet<(String, String)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for declaration in declarations {
        if declaration.implements.is_empty() {
            continue;
        }
        if !matches!(
            declaration.kind,
            DeclarationKind::Struct | DeclarationKind::Unit | DeclarationKind::Extend
        ) || (declaration.kind == DeclarationKind::Extend && declaration.template_extend)
        {
            continue;
        }
        let Some(set) = struct_impls.get(declaration.name.as_str()) else {
            continue;
        };
        let mut sources: SignatureSources = BTreeMap::new();
        let mut stack = Vec::new();
        for interface_name in set {
            interface_signature_sources(
                interface_name.as_str(),
                interfaces,
                &mut stack,
                &mut sources,
            );
        }
        for (method_name, entries) in &sources {
            let Some((a, b)) = conflicting_signature_sources(entries) else {
                continue;
            };
            if reported.insert((declaration.name.clone(), method_name.clone())) {
                diagnostics.push(Diagnostic::new(
                    "E0311",
                    format!(
                        "'{}' implements '{a}' and '{b}' which declare '{method_name}' with different signatures (clause 6.3)",
                        declaration.name
                    ),
                    declaration.name_span.0,
                    declaration.name_span.1,
                ));
            }
        }
    }
}

fn check_fields(
    declaration: &Declaration,
    declared: &BTreeSet<&str>,
    struct_fields: &BTreeMap<&str, Vec<(&str, &TypeExpr)>>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Field names may repeat across `when` branches (each subtype variant
    // carries its own field set, possibly shadowing the base field), so
    // only the top-level scope participates in the duplicate check.
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for member in &declaration.members {
        let Member::Field(field) = member else {
            continue;
        };
        if !seen.insert(field.name.as_str()) {
            diagnostics.push(Diagnostic::new(
                "E0202",
                format!("duplicate field '{}'", field.name),
                field.name_span.0,
                field.name_span.1,
            ));
        }
    }
    // The declaration's own fields are the `typeof()` scope of its field
    // types (5.9): a field type may name any sibling field, `when`
    // branches included.
    let mut own_fields: BTreeMap<&str, &TypeExpr> = BTreeMap::new();
    for_each_field(&declaration.members, &mut |field| {
        own_fields.insert(field.name.as_str(), &field.ty);
    });
    for_each_field(&declaration.members, &mut |field| {
        check_named_type(&field.ty, declared, diagnostics);
        check_long_subtype(&field.ty, false, diagnostics);
        let scope = TypeScope {
            locals: &own_fields,
            own_fields: &own_fields,
            struct_fields,
        };
        check_type_static_types(&field.ty, &scope, diagnostics);
        if let TypeExpr::Scalar {
            base: crate::ast::ScalarBase::Uint | crate::ast::ScalarBase::Int,
            width: Some(width),
            ..
        } = &field.ty
        {
            check_width(width, diagnostics);
        }
        check_field_prefixes(field, diagnostics);
    });
}

/// Clause 6.9.1/6.9.4 field-prefix rules over one parsed field.
///
/// - `!` and `~` are mutually exclusive (`E0213`). The diagnostic points
///   at the field name — the declaration carrying the conflicting
///   prefixes.
/// - A range-generated (`~`) field is not `const` (`E0214`), for the
///   same reason: a const field holds one initialised value, while a
///   `~` field's reduced set is produced per generation (clause 6.9.4,
///   6.9.7).
/// - An init value is not allowed on a generated field (`E0211`): a
///   field without `!` (and without `const`; `static` is the later
///   static-members stage) has its value produced by `gen`, so a
///   written initial value would never survive. The diagnostic points
///   at the init expression. A `~` field is generated too, so the same
///   rule covers it.
fn check_field_prefixes(field: &Field, diagnostics: &mut Vec<Diagnostic>) {
    if field.no_gen && field.range_gen {
        diagnostics.push(Diagnostic::new(
            "E0213",
            format!(
                "'!' and '~' cannot be combined on field '{}' (clause 6.9.1)",
                field.name
            ),
            field.name_span.0,
            field.name_span.1,
        ));
    }
    if field.range_gen && field.is_const {
        diagnostics.push(Diagnostic::new(
            "E0214",
            format!(
                "range-generated field '{}' cannot be const (clause 6.9.4)",
                field.name
            ),
            field.name_span.0,
            field.name_span.1,
        ));
    }
    if let Some(init) = &field.init
        && !field.no_gen
        && !field.is_const
    {
        let span = init.span();
        diagnostics.push(Diagnostic::new(
            "E0211",
            format!(
                "initial value is not allowed on a generated field (declare it '!'): field '{}'",
                field.name
            ),
            span.0,
            span.1,
        ));
    }
}

/// A `type` declaration may not name the same enumeral twice.
fn check_enumerals(declaration: &Declaration, diagnostics: &mut Vec<Diagnostic>) {
    if declaration.kind != DeclarationKind::Type {
        return;
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for member in &declaration.members {
        let Member::Enumeral(enumeral) = member else {
            continue;
        };
        if !seen.insert(enumeral.name.as_str()) {
            diagnostics.push(Diagnostic::new(
                "E0206",
                format!(
                    "duplicate enumeral '{}' in type '{}'",
                    enumeral.name, declaration.name
                ),
                enumeral.name_span.0,
                enumeral.name_span.1,
            ));
        }
    }
}

fn check_methods(
    declaration: &Declaration,
    declared: &BTreeSet<&str>,
    struct_fields: &BTreeMap<&str, Vec<(&str, &TypeExpr)>>,
    iface: &IfaceCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // The declaration's own fields join every method's `typeof()` scope
    // (5.9); the method's parameters shadow them.
    let mut own_fields: BTreeMap<&str, &TypeExpr> = BTreeMap::new();
    for_each_field(&declaration.members, &mut |field| {
        own_fields.insert(field.name.as_str(), &field.ty);
    });
    for_each_method(&declaration.members, &mut |method| {
        // Parameters and local variables share one namespace in this subset;
        // block scoping is a later milestone.
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut locals: BTreeMap<&str, &TypeExpr> = own_fields.clone();
        for param in &method.params {
            if !seen.insert(param.name.as_str()) {
                diagnostics.push(Diagnostic::new(
                    "E0206",
                    format!(
                        "duplicate parameter '{}' in method '{}'",
                        param.name, method.name
                    ),
                    param.name_span.0,
                    param.name_span.1,
                ));
            }
            if let Some(ty) = &param.ty {
                check_named_type(ty, declared, diagnostics);
                check_long_subtype(ty, false, diagnostics);
                locals.insert(param.name.as_str(), ty);
            }
        }
        if let Some(return_type) = &method.return_type {
            check_named_type(return_type, declared, diagnostics);
            check_long_subtype(return_type, false, diagnostics);
            let scope = TypeScope {
                locals: &locals,
                own_fields: &own_fields,
                struct_fields,
            };
            check_type_static_types(return_type, &scope, diagnostics);
        }
        check_statements(
            &method.body,
            &mut seen,
            &StatementScope {
                declared,
                struct_fields,
                own_fields: &own_fields,
                iface,
            },
            &mut locals,
            &method.name,
            diagnostics,
        );
    });
}

/// Walk a statement list. `seen` accumulates local variable names across
/// nested `if` bodies — they are treated as one flat scope for now.
/// 11.3's restriction: an `on` action block shall not contain any
/// time-consuming actions or TCMs — `wait`, temporal `wait`, `sync`,
/// `start` and `compute` at any depth of the block are rejected.
fn check_on_body_time_consumption(
    statements: &[Stmt],
    on_path: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for statement in statements {
        let offending: Option<&str> = match statement {
            Stmt::Wait { .. } | Stmt::WaitTemporal { .. } | Stmt::Sync { .. } => {
                Some("a time-consuming action")
            }
            Stmt::Start { .. } | Stmt::Compute { .. } => Some("a TCM invocation"),
            _ => None,
        };
        if let Some(what) = offending {
            let span = statement.span();
            diagnostics.push(Diagnostic::new(
                "E0210",
                format!(
                    "'{on_path}' on block contains {what}; on action blocks shall not contain time-consuming actions or TCMs (11.3)"
                ),
                span.0,
                span.1,
            ));
        }
        match statement {
            Stmt::If {
                then_body,
                else_body,
                ..
            } => {
                check_on_body_time_consumption(then_body, on_path, diagnostics);
                if let Some(else_body) = else_body {
                    check_on_body_time_consumption(else_body, on_path, diagnostics);
                }
            }
            Stmt::While { body, .. } | Stmt::ForEach { body, .. } | Stmt::ForTo { body, .. } => {
                check_on_body_time_consumption(body, on_path, diagnostics);
            }
            Stmt::Case { arms, default, .. } => {
                for (_, body) in arms {
                    check_on_body_time_consumption(body, on_path, diagnostics);
                }
                if let Some(default) = default {
                    check_on_body_time_consumption(default, on_path, diagnostics);
                }
            }
            _ => {}
        }
    }
}

/// The scope tables a statement walk checks against: the file's declared
/// type names, the per-declaration field tables (`typeof()` member-path
/// resolution, 5.9), the enclosing declaration's own fields
/// (`me.field`), and the interface tables of the interface-assignment
/// rule (5.4.4.4).
struct StatementScope<'a> {
    declared: &'a BTreeSet<&'a str>,
    struct_fields: &'a BTreeMap<&'a str, Vec<(&'a str, &'a TypeExpr)>>,
    own_fields: &'a BTreeMap<&'a str, &'a TypeExpr>,
    iface: &'a IfaceCtx<'a>,
}

/// The interface tables the statement walk consults for the
/// interface-assignment rule (clause 5.4.4.4): which names are
/// interfaces, and whether a struct type implements one.
struct IfaceCtx<'a> {
    interfaces: &'a BTreeMap<&'a str, &'a Declaration>,
    struct_impls: &'a BTreeMap<&'a str, Vec<String>>,
}

impl<'a> IfaceCtx<'a> {
    fn is_interface(&self, name: &str) -> bool {
        self.interfaces.contains_key(name)
    }

    /// Whether struct type `struct_name` implements interface `iface`:
    /// directly (its composed `implementing` set), or through an
    /// implemented interface that inherits it (clause 6.13: an interface
    /// includes all of its bases' members, so implementing a descendant
    /// implements the ancestor).
    fn type_implements(&self, struct_name: &str, iface: &str) -> bool {
        let Some(set) = self.struct_impls.get(struct_name) else {
            return false;
        };
        set.iter()
            .any(|implemented| *implemented == iface || self.interface_is_a(implemented, iface))
    }

    /// Whether interface `name` is `target` or inherits from it.
    fn interface_is_a(&self, name: &str, target: &str) -> bool {
        let mut stack = vec![name.to_owned()];
        let mut seen: BTreeSet<String> = BTreeSet::new();
        while let Some(current) = stack.pop() {
            if !seen.insert(current.clone()) {
                continue;
            }
            if current == target {
                return true;
            }
            if let Some(declaration) = self.interfaces.get(current.as_str()) {
                for (base, _) in &declaration.bases {
                    stack.push(base.clone());
                }
            }
        }
        false
    }
}

fn check_statements<'a>(
    statements: &'a [Stmt],
    seen: &mut BTreeSet<&'a str>,
    scope: &StatementScope<'a>,
    locals: &mut BTreeMap<&'a str, &'a TypeExpr>,
    method_name: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for statement in statements {
        match statement {
            Stmt::VarDecl {
                name,
                name_span,
                ty,
                init,
                ..
            } => {
                if !seen.insert(name.as_str()) {
                    diagnostics.push(Diagnostic::new(
                        "E0206",
                        format!("duplicate local variable '{name}' in method '{method_name}'"),
                        name_span.0,
                        name_span.1,
                    ));
                }
                if let Some(ty) = ty {
                    check_named_type(ty, scope.declared, diagnostics);
                    check_long_subtype(ty, false, diagnostics);
                    let type_scope = TypeScope {
                        locals,
                        own_fields: scope.own_fields,
                        struct_fields: scope.struct_fields,
                    };
                    check_type_static_types(ty, &type_scope, diagnostics);
                    check_interface_var_initializer(ty, init, scope, diagnostics);
                    // A later `typeof(x)` sees this variable's declared
                    // type; an inferred type (`var x := e`) contributes
                    // nothing and its typeofs stay undetermined (5.9's
                    // static-type reading).
                    locals.insert(name.as_str(), ty);
                }
            }
            Stmt::If {
                then_body,
                else_body,
                ..
            } => {
                check_statements(then_body, seen, scope, locals, method_name, diagnostics);
                if let Some(else_body) = else_body {
                    check_statements(else_body, seen, scope, locals, method_name, diagnostics);
                }
            }
            Stmt::Check { else_body, .. } => {
                check_statements(else_body, seen, scope, locals, method_name, diagnostics);
            }
            Stmt::SeqDo { item, .. } => {
                // The item field must exist in the enclosing scope.
                if !seen.contains(item.as_str()) {
                    diagnostics.push(Diagnostic::new(
                        "E0204",
                        format!("unknown sequence item '{item}' in 'do'"),
                        statement.span().0,
                        statement.span().1,
                    ));
                }
            }
            Stmt::Assign { target, value, .. } => {
                // Clause 5.4.4.4: the automatic struct→interface cast
                // requires the struct's type to implement the interface.
                // Checked for the `x = new S;` shape, where the target's
                // declared type (an interface) and the value's type are
                // both statically known; arbitrary expression values are
                // beyond this pass's expression typing.
                if let Expr::Ident { name, .. } = target
                    && let Some(TypeExpr::Named {
                        name: iface_name, ..
                    }) = locals.get(name.as_str()).copied()
                    && scope.iface.is_interface(iface_name)
                    && let Expr::New {
                        ty: Some(new_ty), ..
                    } = value
                    && let TypeExpr::Named {
                        name: new_name,
                        span: new_span,
                    } = new_ty
                    && !scope.iface.is_interface(new_name)
                    && !scope.iface.type_implements(new_name, iface_name)
                {
                    diagnostics.push(Diagnostic::new(
                        "E0312",
                        format!(
                            "cannot assign '{new_name}' to interface '{iface_name}': '{new_name}' does not implement '{iface_name}' (clause 5.4.4.4)"
                        ),
                        new_span.0,
                        new_span.1,
                    ));
                }
            }
            Stmt::ExprStmt { .. }
            | Stmt::Return { .. }
            | Stmt::While { .. }
            | Stmt::RepeatUntil { .. }
            | Stmt::Break { .. }
            | Stmt::Continue { .. }
            | Stmt::Case { .. }
            | Stmt::Wait { .. }
            | Stmt::WaitTemporal { .. }
            | Stmt::Sync { .. }
            | Stmt::Emit { .. }
            | Stmt::Start { .. }
            | Stmt::Compute { .. }
            | Stmt::StopRun { .. }
            | Stmt::ForEach { .. }
            | Stmt::ForTo { .. }
            | Stmt::Print { .. }
            | Stmt::Gen { .. } => {}
        }
    }
}

/// The static-type scope a `typeof()`/`typeof_item()` argument (5.9)
/// resolves against: the names visible at the type position (the
/// declaration's fields, the method's parameters and the locals declared
/// so far), the enclosing declaration's own fields (`me.field`), and the
/// file's per-declaration field tables (a member path resolves through
/// the named type's fields).
struct TypeScope<'a> {
    locals: &'a BTreeMap<&'a str, &'a TypeExpr>,
    own_fields: &'a BTreeMap<&'a str, &'a TypeExpr>,
    struct_fields: &'a BTreeMap<&'a str, Vec<(&'a str, &'a TypeExpr)>>,
}

/// The static type of a `typeof()`/`typeof_item()` argument against the
/// current minimal type information (5.9): a field path, a local `var`,
/// a method parameter, or a literal. The expression is only inspected,
/// never evaluated. `None` — the static type cannot be determined here
/// (`E0208` at the call site).
fn static_type_of(expr: &Expr, scope: &TypeScope) -> Option<TypeExpr> {
    match expr {
        Expr::Ident { name, .. } => scope.locals.get(name.as_str()).map(|ty| (*ty).clone()),
        Expr::Paren { expr, .. } => static_type_of(expr, scope),
        Expr::MemberAccess { object, name, .. } => {
            // `me.field` / `self.field` — the enclosing declaration's
            // own fields.
            if let Expr::Ident { name: root, .. } = object.as_ref()
                && (root == "me" || root == "self")
            {
                return scope.own_fields.get(name.as_str()).map(|ty| (*ty).clone());
            }
            // A member path resolves through a struct-like type's field
            // table; the chain recurses (`a.b.c`).
            let base = static_type_of(object, scope)?;
            let type_name = struct_like_name(&base)?;
            let fields = scope.struct_fields.get(type_name.as_str())?;
            fields
                .iter()
                .find(|(field, _)| *field == name.as_str())
                .map(|(_, ty)| (*ty).clone())
        }
        // A literal's static type is its predefined scalar base; the
        // unsized and sized numeric literals are `int` at this stage.
        Expr::IntLiteral { .. } | Expr::BitLiteral { .. } => Some(scalar_type(ScalarBase::Int)),
        Expr::RealLiteral { .. } => Some(scalar_type(ScalarBase::Real)),
        Expr::BoolLiteral { .. } => Some(scalar_type(ScalarBase::Bool)),
        Expr::StringLiteral { .. } => Some(scalar_type(ScalarBase::String)),
        _ => None,
    }
}

/// The name of the struct-like type a type expression denotes, for the
/// member-path resolution (`s` in `typeof(x.n)` where `x: s`).
fn struct_like_name(ty: &TypeExpr) -> Option<String> {
    match ty {
        TypeExpr::Named { name, .. } => Some(name.clone()),
        TypeExpr::WhenQualified { base, .. } => struct_like_name(base),
        _ => None,
    }
}

fn scalar_type(base: ScalarBase) -> TypeExpr {
    TypeExpr::Scalar {
        base,
        width: None,
        span: (
            crate::lexer::Position {
                byte: 0,
                line: 0,
                column: 0,
            },
            crate::lexer::Position {
                byte: 0,
                line: 0,
                column: 0,
            },
        ),
    }
}

/// Check the `typeof()`/`typeof_item()` leaves of a type position (5.9):
/// the argument's static type must be determinable (`E0208`), and
/// `typeof_item()`'s argument must be a list (`E0209`).
fn check_type_static_types(ty: &TypeExpr, scope: &TypeScope, diagnostics: &mut Vec<Diagnostic>) {
    match ty {
        TypeExpr::TypeOf { expr, span } => {
            if static_type_of(expr, scope).is_none() {
                diagnostics.push(Diagnostic::new(
                    "E0208",
                    "typeof(): cannot determine the static type of this expression",
                    span.0,
                    span.1,
                ));
            }
        }
        TypeExpr::TypeOfItem { expr, span } => match static_type_of(expr, scope) {
            None => diagnostics.push(Diagnostic::new(
                "E0208",
                "typeof_item(): cannot determine the static type of this expression",
                span.0,
                span.1,
            )),
            Some(resolved) if !matches!(resolved, TypeExpr::List { .. }) => {
                diagnostics.push(Diagnostic::new(
                    "E0209",
                    "typeof_item(): the static type of this expression is not a list",
                    span.0,
                    span.1,
                ));
            }
            Some(_) => {}
        },
        TypeExpr::List { element, .. } => check_type_static_types(element, scope, diagnostics),
        TypeExpr::WhenQualified { base, .. } => check_type_static_types(base, scope, diagnostics),
        TypeExpr::TemplateRef { args, .. } => {
            for arg in args {
                check_type_static_types(arg, scope, diagnostics);
            }
        }
        _ => {}
    }
}

fn check_named_type(ty: &TypeExpr, declared: &BTreeSet<&str>, diagnostics: &mut Vec<Diagnostic>) {
    match ty {
        TypeExpr::Named { name, span } => {
            if !declared.contains(name.as_str()) && !crate::builtins::is_builtin_type(name) {
                diagnostics.push(Diagnostic::new(
                    "E0203",
                    format!("unknown type '{name}'"),
                    span.0,
                    span.1,
                ));
            }
        }
        TypeExpr::List { element, .. } => check_named_type(element, declared, diagnostics),
        // A when-qualified type resolves if its base resolves (6.7 —
        // the subtype itself is checked at elaboration).
        TypeExpr::WhenQualified { base, .. } => check_named_type(base, declared, diagnostics),
        // A template instantiation checks its template name and each
        // argument; a formal parameter is bound by the enclosing
        // template declaration (checked at instantiation, a later
        // milestone).
        TypeExpr::TemplateRef { name, args, span } => {
            if !declared.contains(name.as_str()) {
                diagnostics.push(Diagnostic::new(
                    "E0203",
                    format!("unknown template '{name}'"),
                    span.0,
                    span.1,
                ));
            }
            for arg in args {
                check_named_type(arg, declared, diagnostics);
            }
        }
        TypeExpr::Formal { .. } => {}
        // A `typeof()`/`typeof_item()` placeholder's argument is an
        // expression, not a type; its resolution is 5.9's
        // `check_type_static_types`, and the resolved type's own name is
        // validated after elaboration rewrites it.
        TypeExpr::TypeOf { .. } | TypeExpr::TypeOfItem { .. } => {}
        TypeExpr::Scalar { .. } | TypeExpr::InlineEnum { .. } => {}
    }
}

/// Clause 5.4.4.4: assigning a struct value to an interface-typed target
/// is an automatic cast only when the struct's type implements the
/// interface (directly or through inheritance). Checked for the
/// `var x: I = new S;` shape — the one value form whose static type this
/// pass knows. The diagnostic points at the `new` type's name.
fn check_interface_var_initializer(
    ty: &TypeExpr,
    init: &Option<Expr>,
    scope: &StatementScope,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let TypeExpr::Named {
        name: iface_name, ..
    } = ty
    else {
        return;
    };
    if !scope.iface.is_interface(iface_name) {
        return;
    }
    let Some(Expr::New {
        ty: Some(new_ty), ..
    }) = init
    else {
        return;
    };
    let TypeExpr::Named {
        name: new_name,
        span: new_span,
    } = new_ty
    else {
        return;
    };
    if scope.iface.is_interface(new_name) || scope.iface.type_implements(new_name, iface_name) {
        return;
    }
    diagnostics.push(Diagnostic::new(
        "E0312",
        format!(
            "cannot assign '{new_name}' to interface '{iface_name}': '{new_name}' does not implement '{iface_name}' (clause 5.4.4.4)"
        ),
        new_span.0,
        new_span.1,
    ));
}

/// A literal-zero width is always wrong; non-literal widths are a later
/// (elaboration) concern and are not checked.
fn check_width(width: &Expr, diagnostics: &mut Vec<Diagnostic>) {
    let Expr::IntLiteral { text, span } = width else {
        return;
    };
    if text.chars().all(|c| c == '0' || c == '_') {
        diagnostics.push(Diagnostic::new(
            "E0205",
            "width must be positive",
            span.0,
            span.1,
        ));
    }
}

/// Subtypes shall not be derived from `longint`/`longuint` (clause
/// 5.2.3.1): a `(bits: N)` width specification on either long base, or a
/// range qualification over a `type` declaration of one, is reported
/// once, at the long base's own token. A bare alias
/// (`type t: longint;`) is not a subtype and stays legal.
fn check_long_subtype(ty: &TypeExpr, ranged: bool, diagnostics: &mut Vec<Diagnostic>) {
    let TypeExpr::Scalar { base, width, span } = ty else {
        return;
    };
    if !matches!(
        base,
        crate::ast::ScalarBase::Longint | crate::ast::ScalarBase::Longuint
    ) || !(width.is_some() || ranged)
    {
        return;
    }
    diagnostics.push(Diagnostic::new(
        "E0207",
        format!("subtype of '{}' is not allowed", base.name()),
        span.0,
        span.1,
    ));
}

/// Report root identifiers of the constraint expression that are neither
/// fields of the enclosing declaration nor declared enumerals. A call's
/// callee names a method rather than a field, so only its arguments
/// contribute roots.
fn check_constraint_fields(
    constraint: &KeepConstraint,
    field_names: &BTreeSet<&str>,
    enumerals: &BTreeSet<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Bind constraints name HDL-side signals, not e fields; skip them.
    if constraint.is_bind {
        return;
    }
    let mut roots = Vec::new();
    collect_root_identifiers(&constraint.expr, &mut roots);
    for (name, span) in roots {
        if name == "me" || name == "self" {
            continue;
        }
        if !field_names.contains(name.as_str()) && !enumerals.contains(name.as_str()) {
            diagnostics.push(Diagnostic::new(
                "E0204",
                format!("unknown identifier '{name}' in keep constraint"),
                span.0,
                span.1,
            ));
        }
    }
}

fn collect_root_identifiers(expr: &Expr, roots: &mut Vec<(String, crate::ast::Span)>) {
    match expr {
        Expr::Ident { name, span } => roots.push((name.clone(), *span)),
        Expr::MemberAccess { object, .. } => collect_root_identifiers(object, roots),
        Expr::Call { args, .. } => {
            for arg in args {
                collect_root_identifiers(arg, roots);
            }
        }
        Expr::Index { base, index, .. } => {
            collect_root_identifiers(base, roots);
            collect_root_identifiers(index, roots);
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_root_identifiers(lhs, roots);
            collect_root_identifiers(rhs, roots);
        }
        Expr::Unary { expr, .. } | Expr::Paren { expr, .. } => {
            collect_root_identifiers(expr, roots)
        }
        // Quoted HDL paths are simulator signals, not struct fields.
        Expr::Ternary {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_root_identifiers(cond, roots);
            collect_root_identifiers(then_expr, roots);
            collect_root_identifiers(else_expr, roots);
        }
        Expr::CastAs { expr, .. } => collect_root_identifiers(expr, roots),
        Expr::BitSlice {
            base, high, low, ..
        } => {
            collect_root_identifiers(base, roots);
            collect_root_identifiers(high, roots);
            collect_root_identifiers(low, roots);
        }
        Expr::InSet {
            element,
            set: crate::ast::InSetExpr::Items(items),
            ..
        } => {
            collect_root_identifiers(element, roots);
            for item in items {
                collect_root_identifiers(item, roots);
            }
        }
        Expr::InSet {
            element,
            set: crate::ast::InSetExpr::Range(low, high),
            ..
        } => {
            collect_root_identifiers(element, roots);
            collect_root_identifiers(low, roots);
            collect_root_identifiers(high, roots);
        }
        Expr::IsA { expr, .. } => collect_root_identifiers(expr, roots),
        // An `in_table` test's tuple elements and row elements all name
        // values (a type-set element names a type, not a variable).
        Expr::InTable { tuple, rows, .. } => {
            for element in tuple {
                collect_root_identifiers(element, roots);
            }
            for row in rows {
                for element in row {
                    match element {
                        crate::ast::TableElem::Scalar(expr) => {
                            collect_root_identifiers(expr, roots)
                        }
                        crate::ast::TableElem::Set(crate::ast::InSetExpr::Items(items)) => {
                            for item in items {
                                collect_root_identifiers(item, roots);
                            }
                        }
                        crate::ast::TableElem::Set(crate::ast::InSetExpr::Range(low, high)) => {
                            collect_root_identifiers(low, roots);
                            collect_root_identifiers(high, roots);
                        }
                        crate::ast::TableElem::TypeSet { .. } => {}
                    }
                }
            }
        }
        // The actions of an `evaluate` block contribute the roots of the
        // expressions they evaluate (assigned values and statement
        // expressions); the block's own variable is scoped to it, so
        // references to that name inside the block resolve to the
        // variable, not to an outer field of the same name.
        Expr::Evaluate { var, body, .. } => {
            let mut inner_roots = Vec::new();
            collect_statement_roots(body, &mut inner_roots);
            let var_name = var.as_deref().unwrap_or("value");
            roots.extend(inner_roots.into_iter().filter(|(name, _)| name != var_name));
        }
        // A subtype-qualified access's roots are its base's; the
        // qualifier words are subtype values, not references.
        Expr::QualifiedField { base, .. } => collect_root_identifiers(base, roots),
        // A select distribution's weights are the only AST side; the
        // choices are recorded as raw tokens.
        Expr::Select { arms, .. } => {
            for arm in arms {
                collect_root_identifiers(&arm.weight, roots);
            }
        }
        // `new` creates an instance; its type names no field and its
        // init assignments run against the fresh instance.
        Expr::New { inits, .. } => {
            for statement in inits {
                collect_with_init_roots(statement, roots);
            }
        }
        // A set_of_values argument names a type, not a variable.
        Expr::SetOfValues { .. } => {}
        // A list concat's elements are ordinary sub-expressions.
        Expr::ListConcat { items, .. } => {
            for item in items {
                collect_root_identifiers(item, roots);
            }
        }
        Expr::HdlPath { .. } => {}
        Expr::IntLiteral { .. }
        | Expr::RealLiteral { .. }
        | Expr::BitLiteral { .. }
        | Expr::BoolLiteral { .. }
        | Expr::StringLiteral { .. } => {}
    }
}

/// Visit every field of the declaration, descending into `when` subtype
/// members.
/// Root identifiers of the expressions inside a `new ... with {}`
/// block: the assigned values, not the fresh instance's fields being
/// assigned to.
fn collect_with_init_roots(statement: &Stmt, roots: &mut Vec<(String, crate::ast::Span)>) {
    match statement {
        Stmt::Assign { value, .. } => collect_root_identifiers(value, roots),
        Stmt::ExprStmt { expr, .. } => collect_root_identifiers(expr, roots),
        _ => {}
    }
}

/// Root identifiers of the expressions a statement list evaluates:
/// assigned values (the targets resolve in the enclosing scope) and
/// statement expressions, descended into compound statements. Used for
/// the action body of an `evaluate` block (clause 4.17.7).
fn collect_statement_roots(statements: &[Stmt], roots: &mut Vec<(String, crate::ast::Span)>) {
    for statement in statements {
        match statement {
            Stmt::Assign { target, value, .. } => {
                collect_root_identifiers(target, roots);
                collect_root_identifiers(value, roots);
            }
            Stmt::ExprStmt { expr, .. } => collect_root_identifiers(expr, roots),
            Stmt::VarDecl {
                init: Some(init), ..
            } => collect_root_identifiers(init, roots),
            Stmt::Return {
                value: Some(value), ..
            } => collect_root_identifiers(value, roots),
            Stmt::If {
                cond,
                then_body,
                else_body,
                ..
            } => {
                collect_root_identifiers(cond, roots);
                collect_statement_roots(then_body, roots);
                if let Some(else_body) = else_body {
                    collect_statement_roots(else_body, roots);
                }
            }
            Stmt::While { cond, body, .. } => {
                collect_root_identifiers(cond, roots);
                collect_statement_roots(body, roots);
            }
            Stmt::RepeatUntil { body, cond, .. } => {
                collect_statement_roots(body, roots);
                collect_root_identifiers(cond, roots);
            }
            Stmt::ForTo { from, to, body, .. } => {
                collect_root_identifiers(from, roots);
                collect_root_identifiers(to, roots);
                collect_statement_roots(body, roots);
            }
            Stmt::ForEach { list, body, .. } => {
                collect_root_identifiers(list, roots);
                collect_statement_roots(body, roots);
            }
            Stmt::Case {
                scrutinee,
                arms,
                default,
                ..
            } => {
                collect_root_identifiers(scrutinee, roots);
                for (alternatives, body) in arms {
                    for alternative in alternatives {
                        collect_root_identifiers(alternative, roots);
                    }
                    collect_statement_roots(body, roots);
                }
                if let Some(default) = default {
                    collect_statement_roots(default, roots);
                }
            }
            _ => {}
        }
    }
}

/// Visit every field of the declaration, descending into `when` subtype
/// members.
fn for_each_field<'a>(members: &'a [Member], f: &mut impl FnMut(&'a Field)) {
    for member in members {
        match member {
            Member::Field(field) => f(field),
            Member::When(when) => for_each_field(&when.members, f),
            _ => {}
        }
    }
}

/// Visit every method, descending into `when` subtype members.
fn for_each_method<'a>(members: &'a [Member], f: &mut impl FnMut(&'a MethodDecl)) {
    for member in members {
        match member {
            Member::Method(method) => f(method),
            Member::When(when) => for_each_method(&when.members, f),
            _ => {}
        }
    }
}

/// Visit every `on` member, descending into `when` subtype members.
fn for_each_on<'a>(members: &'a [Member], f: &mut impl FnMut(&'a crate::ast::OnDecl)) {
    for member in members {
        match member {
            Member::On(on) => f(on),
            Member::When(when) => for_each_on(&when.members, f),
            _ => {}
        }
    }
}

/// Visit every `keep` constraint, descending into `when` subtype members.
fn for_each_constraint<'a>(members: &'a [Member], f: &mut impl FnMut(&'a KeepConstraint)) {
    for member in members {
        match member {
            Member::Constraint(constraint) => f(constraint),
            Member::When(when) => for_each_constraint(&when.members, f),
            _ => {}
        }
    }
}
