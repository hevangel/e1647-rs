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
//!   init value requires `!`, `const`, or `static` — a static field's
//!   value is created at declaration, not generated)
//! - `E0212` a `static const` field without an init value (clause
//!   6.9.4: a const field is initialised once; a static field's one
//!   initialisation is the declaration's init value)
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
//! - `E0215` `me`/`self` used inside a static method (clause 17.2.2: a
//!   static method has no instance; the same rule covers the static
//!   `on` forms of clause 11.2)
//! - `E0216` a generation/check/message action (`gen`, `check that`,
//!   `dut_error`/`dut_errorf`, `message`/`messagef`) inside a static
//!   method (clauses 10.6.2, 16.3.2.1, 16.3.3, 16.3.4, 24.4.1; the
//!   same restriction applies in the static `on` forms)
//! - `E0217` reserved ("static method 'f' can only be extended in the
//!   struct that declares it" is reported as `E0309`, see below)
//! - `E0218` a keyed-list key that is a static field or of list/set
//!   type (clause 5.2.11; a struct-typed key may contain static
//!   fields)
//!
//! The annotation checks of the 2019 annotations stage (IEEE 1647-2019
//! clause 35) run here and, for the module-scope rule, from the parser:
//!
//! - `E0137` a member of an `annotation @name { .. }` body that is
//!   neither a field nor a method (clause 35.2)
//! - `E0138` an annotation field whose type is not scalar/string/set
//!   (clause 35.2; see [`check_annotation_body`] for the reading)
//! - `E0139` a second attachment of a non-`repeatable` annotation type
//!   on one entity (clause 35.3)
//! - `E0320` an annotation type extended (`extend @name`/`extend
//!   name`) or inherited from (`like`) (clause 35.2)
//! - `E0321` an annotation type used in the module that declares it —
//!   raised by [`check_annotation_module_scope`], which the parser
//!   calls once per file: every file is one module in this
//!   implementation (an imported file is spliced as whole
//!   declarations), so the same-file restriction must be evaluated
//!   before the multi-file merge (clause 35.2)
//! - `E0140` an annotation attached to a `when` block (clause 35.3) —
//!   raised by the parser, which sees the attachment directly
//!
//! Further static-member restrictions of the 2019 static-members stage
//! (IEEE 1647-2019 clauses 4.17.4, 6.9.3, 7.2.2, 7.2.4, 17.2.4) are
//! raised from this pass under their elaboration-range codes — the
//! check range `E0207`–`E0219` was exhausted when they were allocated —
//! and the fixture harness and the CLI's `parse` command stop at parse
//! + check, so the diagnostics must be visible from this pass; the
//!   codes keep their plan allocation:
//!
//! - `E0304` a static field, static method or static-context `on`
//!   block declared inside a `when` subtype (clauses 6.9.3, 17.2.2,
//!   11.2: a static member belongs to the type itself, and a `when`
//!   subtype is not a type of its own)
//! - `E0305` a static field declared physical (`%`) or
//!   range-generated (`~`) (clause 6.9.3: a static field's one value
//!   is neither packed nor generated per instance)
//! - `E0306` a static field of unit-instance type, or a list of unit
//!   instances (clauses 6.9.3, 7.2.2, 7.2.4: a unit instance belongs
//!   to its parent instance)
//! - `E0307` `Type::member` where `member` is not a static member
//!   declared in exactly `Type` (clause 4.17.4: not a parent's, and a
//!   plain instance member does not qualify either)
//! - `E0309` a static method extended (`is also`/`is first`/`is
//!   only`) outside the struct that declares it (clause 17.2.4)
//! - `E0310` a static event declared inside a `when` subtype (clause
//!   11.1.4.1; the dedicated code the plan reserves for it)
//!
//! `E0308` ("static member cannot be added to 'global'") stays
//! reserved and is unreachable in this implementation: `extend global`
//! is rejected earlier, by the parser (`E0116`, clause 4.4.2.2).
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
    // The annotation types (clause 35.2): a struct-like kind in the type
    // namespace. Their names resolve in every type position (the
    // same-module restriction is E0321, a per-file pass — see
    // [`check_annotation_module_scope`]); their extend/like targets are
    // rejected (E0320).
    let annotation_decls: BTreeMap<&str, &Declaration> = declarations
        .iter()
        .filter(|declaration| matches!(declaration.kind, DeclarationKind::Annotation { .. }))
        .map(|declaration| (declaration.name.as_str(), declaration))
        .collect();
    for name in annotation_decls.keys() {
        declared.insert(name);
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
    // The declaration-kind map of the unit-instance-type restriction
    // (E0306): which named types are `unit` declarations.
    let kinds: BTreeMap<&str, DeclarationKind> = declarations
        .iter()
        .filter(|declaration| declaration.kind != DeclarationKind::Extend)
        .map(|declaration| (declaration.name.as_str(), declaration.kind))
        .collect();
    // The static-member namespace a `Type::member` access resolves
    // against (T2.1–T2.3): per type name, the static fields, methods
    // and events its base declaration and `extend` blocks declare.
    let static_members = static_member_names(declarations);
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
        check_extend_target(
            declaration,
            &declared,
            &interfaces,
            &annotation_decls,
            &mut diagnostics,
        );
        check_like_target(
            declaration,
            &declared,
            &interfaces,
            &annotation_decls,
            &mut diagnostics,
        );
        check_annotation_body(declaration, &kinds, &mut diagnostics);
        check_annotation_repeats(declaration, &annotation_decls, &mut diagnostics);
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
        check_fields(
            declaration,
            &declared,
            &struct_fields,
            declarations,
            &kinds,
            &static_members,
            &mut diagnostics,
        );
        check_enumerals(declaration, &mut diagnostics);
        check_methods(
            declaration,
            &declared,
            &struct_fields,
            &iface,
            &static_members,
            &mut diagnostics,
        );
        check_static_members_in_whens(declaration, &mut diagnostics);
        check_static_method_extensions(declaration, declarations, &mut diagnostics);
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
            // The static `on` forms (`static on <event>`, `on
            // Type::event`) run with no instance: the static-context
            // restrictions of T2.2 apply, and every `Type::member`
            // access is validated.
            if on.is_static || on.type_qualified.is_some() {
                check_static_context_rules(&on.body, "a static 'on' block", &mut diagnostics);
            }
            check_body_static_accesses(&on.body, &static_members, &mut diagnostics);
        });
        // A `keep` member inside an annotation body is already E0137
        // (not a field or method); walking its expression for the
        // enclosing type's fields would only add noise.
        if declaration.kind != DeclarationKind::Extend
            && !matches!(declaration.kind, DeclarationKind::Annotation { .. })
        {
            let mut field_names: BTreeSet<&str> = BTreeSet::new();
            for_each_field(&declaration.members, &mut |field| {
                field_names.insert(field.name.as_str());
            });
            for_each_constraint(&declaration.members, &mut |constraint| {
                check_constraint_fields(constraint, &field_names, &enumerals, &mut diagnostics);
                check_expr_static_accesses(&constraint.expr, &static_members, &mut diagnostics);
            });
        }
    }
    diagnostics
}

fn check_extend_target(
    declaration: &Declaration,
    declared: &BTreeSet<&str>,
    interfaces: &BTreeMap<&str, &Declaration>,
    annotation_decls: &BTreeMap<&str, &Declaration>,
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
    // Clause 35.2: an annotation type cannot be extended — its members
    // are fixed at declaration.
    if annotation_decls.contains_key(declaration.name.as_str()) {
        diagnostics.push(Diagnostic::new(
            "E0320",
            format!(
                "annotation type '{}' cannot be extended (clause 35.2)",
                declaration.name
            ),
            declaration.name_span.0,
            declaration.name_span.1,
        ));
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
        // `any_unit` is the predefined base unit whose members compose
        // into every unit (the 24.9.2.14 message hooks are extendable on
        // it); other builtin bases have no composed members here and
        // stay rejected as extend targets.
        && !crate::builtins::EXTENDABLE_BASES.contains(&declaration.name.as_str())
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
    annotation_decls: &BTreeMap<&str, &Declaration>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(like) = &declaration.like else {
        return;
    };
    let like_span = declaration.like_span.unwrap_or(declaration.name_span);
    // Clause 35.2: an annotation type cannot be inherited from — the
    // annotation's members are fixed at declaration.
    if annotation_decls.contains_key(like.as_str()) {
        diagnostics.push(Diagnostic::new(
            "E0320",
            format!("annotation type '{like}' cannot be inherited from (clause 35.2)"),
            like_span.0,
            like_span.1,
        ));
        return;
    }
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

/// Clause 35.2 annotation-body restrictions. The members of an
/// `annotation @name { .. }` body are fields and methods only
/// (`E0137`); a field's type is a scalar or string base, an inline
/// enumerand set, or a named `type` declaration — the clause's
/// "scalar, string or set" reading, where a named scalar subtype
/// (`type addr_t: int(bits: 8)`) counts as scalar and a named
/// enumerand type as set (`E0138`). The restrictions the migration
/// plan allocates no code for are reported under the parser's
/// unsupported-member code `E0113` with their own messages: a `static`
/// field, an access-modifier prefix, an extended `init()` and a `new`
/// method. All annotation fields are implicitly `const` — recorded by
/// the checker's leniency toward init values, not rewritten on the AST.
fn check_annotation_body(
    declaration: &Declaration,
    kinds: &BTreeMap<&str, DeclarationKind>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !matches!(declaration.kind, DeclarationKind::Annotation { .. }) {
        return;
    }
    for member in &declaration.members {
        match member {
            Member::Field(field) => {
                if !annotation_field_type_allowed(&field.ty, kinds) {
                    diagnostics.push(Diagnostic::new(
                        "E0138",
                        format!(
                            "annotation field '{}' must be scalar, string or set (clause 35.2)",
                            field.name
                        ),
                        field.name_span.0,
                        field.name_span.1,
                    ));
                }
                if field.is_static {
                    diagnostics.push(Diagnostic::new(
                        "E0113",
                        format!(
                            "a static field is not allowed in an annotation body: '{}' (clause 35.2)",
                            field.name
                        ),
                        field.name_span.0,
                        field.name_span.1,
                    ));
                }
                if field.visibility.is_some() {
                    diagnostics.push(Diagnostic::new(
                        "E0113",
                        format!(
                            "an access modifier is not allowed on annotation member '{}' (clause 35.2)",
                            field.name
                        ),
                        field.name_span.0,
                        field.name_span.1,
                    ));
                }
            }
            Member::Method(method) => {
                if method.name == "new" {
                    diagnostics.push(Diagnostic::new(
                        "E0113",
                        "the 'new' method is not allowed in an annotation body (clause 35.2)",
                        method.name_span.0,
                        method.name_span.1,
                    ));
                }
                if method.name == "init" && method.modifier.is_some() {
                    diagnostics.push(Diagnostic::new(
                        "E0113",
                        "init() of an annotation cannot be extended (clause 35.2)",
                        method.name_span.0,
                        method.name_span.1,
                    ));
                }
            }
            other => {
                let span = annotation_member_span(other);
                diagnostics.push(Diagnostic::new(
                    "E0137",
                    "an annotation member must be a field or method (clause 35.2)",
                    span.0,
                    span.1,
                ));
            }
        }
    }
}

/// Whether an annotation field's type is one of clause 35.2's scalar,
/// string or set shapes.
fn annotation_field_type_allowed(ty: &TypeExpr, kinds: &BTreeMap<&str, DeclarationKind>) -> bool {
    match ty {
        // Any scalar base, `string` included (Table 17).
        TypeExpr::Scalar { .. } => true,
        // `[a, b]` — an inline enumerand set.
        TypeExpr::InlineEnum { .. } => true,
        // A named type is legal when it names a `type` declaration: an
        // enumerand set or a scalar subtype. Structs, units, interfaces
        // and other annotation types are not.
        TypeExpr::Named { name, .. } => kinds.get(name.as_str()) == Some(&DeclarationKind::Type),
        _ => false,
    }
}

/// The span an annotation-body diagnostic points at.
fn annotation_member_span(member: &Member) -> crate::ast::Span {
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
        Member::Method(method) => method.name_span,
    }
}

/// Clause 35.3: several annotations may precede one entity, and more
/// than one of the same type only if that type is `repeatable`
/// (`E0139`). An attachment of an annotation type this declaration set
/// does not declare carries no repeatability information; no code is
/// allocated for an unknown annotation type, so it is not reported.
fn check_annotation_repeats(
    declaration: &Declaration,
    annotation_decls: &BTreeMap<&str, &Declaration>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    check_annotation_entity_repeats(&declaration.annotations, annotation_decls, diagnostics);
    check_member_annotation_repeats(&declaration.members, annotation_decls, diagnostics);
}

fn check_member_annotation_repeats(
    members: &[Member],
    annotation_decls: &BTreeMap<&str, &Declaration>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for member in members {
        if let Member::When(when) = member {
            check_member_annotation_repeats(&when.members, annotation_decls, diagnostics);
            continue;
        }
        check_annotation_entity_repeats(member_annotations(member), annotation_decls, diagnostics);
    }
}

fn check_annotation_entity_repeats(
    annotations: &[crate::ast::Annotation],
    annotation_decls: &BTreeMap<&str, &Declaration>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut seen_non_repeatable: BTreeSet<&str> = BTreeSet::new();
    for annotation in annotations {
        let Some(decl) = annotation_decls.get(annotation.name.as_str()) else {
            continue;
        };
        let DeclarationKind::Annotation { repeatable } = decl.kind else {
            continue;
        };
        if !repeatable && !seen_non_repeatable.insert(annotation.name.as_str()) {
            diagnostics.push(Diagnostic::new(
                "E0139",
                format!(
                    "annotation '@{}' is not repeatable (clause 35.3)",
                    annotation.name
                ),
                annotation.span.0,
                annotation.span.1,
            ));
        }
    }
}

/// The annotations attached to one member, for the repeat check.
fn member_annotations(member: &Member) -> &[crate::ast::Annotation] {
    match member {
        Member::Field(field) => &field.annotations,
        Member::Constraint(constraint) => &constraint.annotations,
        Member::Method(method) => &method.annotations,
        Member::Event(event) => &event.annotations,
        Member::On(on) => &on.annotations,
        Member::Cover(cover) => &cover.annotations,
        Member::Expect(expect) => &expect.annotations,
        Member::Port(port) => &port.annotations,
        Member::Macro(macro_decl) => &macro_decl.annotations,
        Member::Enumeral(_) | Member::When(_) | Member::Import { .. } | Member::Define { .. } => {
            &[]
        }
    }
}

/// Clause 35.2: a newly declared annotation type cannot be used in the
/// module that declares it — only from the next module. In this
/// implementation every file is one module (the CLI splices an imported
/// file's whole declarations before the importing file's own, so a
/// merged-set check could not tell the files apart), and the parser
/// calls this pass once per file, where the file boundary is known: any
/// use of an annotation type declared in the same file — as a type
/// (field, method parameter/return, local `var`) or as an attachment —
/// is reported once per type at its first use (`E0321`). Extend/like
/// targets of an annotation type are the E0320 domain, not uses here;
/// `new` of one is a run-time error (E0402).
pub fn check_annotation_module_scope(declarations: &[Declaration]) -> Vec<Diagnostic> {
    let own_annotations: BTreeSet<&str> = declarations
        .iter()
        .filter(|declaration| matches!(declaration.kind, DeclarationKind::Annotation { .. }))
        .map(|declaration| declaration.name.as_str())
        .collect();
    if own_annotations.is_empty() {
        return Vec::new();
    }
    let mut state = ModuleScopeState {
        own_annotations,
        reported: BTreeSet::new(),
        diagnostics: Vec::new(),
    };
    for declaration in declarations {
        state.annotation_uses(&declaration.annotations);
        for member in &declaration.members {
            state.member_uses(member);
        }
        for_each_field(&declaration.members, &mut |field| {
            state.type_uses(&field.ty);
        });
        for_each_method(&declaration.members, &mut |method| {
            for param in &method.params {
                if let Some(ty) = &param.ty {
                    state.type_uses(ty);
                }
            }
            if let Some(ty) = &method.return_type {
                state.type_uses(ty);
            }
            state.statement_uses(&method.body);
        });
        for_each_on(&declaration.members, &mut |on| {
            state.statement_uses(&on.body);
        });
        for member in &declaration.members {
            if let Member::Expect(expect) = member {
                state.statement_uses(&expect.else_body);
            }
        }
    }
    state.diagnostics
}

struct ModuleScopeState<'a> {
    own_annotations: BTreeSet<&'a str>,
    reported: BTreeSet<&'a str>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> ModuleScopeState<'a> {
    fn report(&mut self, name: &'a str, span: crate::ast::Span) {
        if self.own_annotations.contains(name) && self.reported.insert(name) {
            self.diagnostics.push(Diagnostic::new(
                "E0321",
                format!(
                    "annotation type '{name}' is used in the module that declares it (clause 35.2)"
                ),
                span.0,
                span.1,
            ));
        }
    }

    fn annotation_uses(&mut self, annotations: &'a [crate::ast::Annotation]) {
        for annotation in annotations {
            if self.own_annotations.contains(annotation.name.as_str()) {
                let name = annotation.name.as_str();
                let span = annotation.span;
                self.report(name, span);
            }
        }
    }

    fn member_uses(&mut self, member: &'a Member) {
        if let Member::When(when) = member {
            for inner in &when.members {
                self.member_uses(inner);
            }
        }
        self.annotation_uses(member_annotations(member));
    }

    fn type_uses(&mut self, ty: &'a TypeExpr) {
        match ty {
            TypeExpr::Named { name, span } => {
                let name = name.as_str();
                let span = *span;
                self.report(name, span);
            }
            TypeExpr::List { element, .. } => self.type_uses(element),
            TypeExpr::WhenQualified { base, .. } => self.type_uses(base),
            TypeExpr::TemplateRef { args, .. } => {
                for arg in args {
                    self.type_uses(arg);
                }
            }
            _ => {}
        }
    }

    fn statement_uses(&mut self, statements: &'a [Stmt]) {
        for statement in statements {
            match statement {
                Stmt::VarDecl { ty: Some(ty), .. } => self.type_uses(ty),
                Stmt::If {
                    then_body,
                    else_body: Some(else_body),
                    ..
                } => {
                    self.statement_uses(then_body);
                    self.statement_uses(else_body);
                }
                Stmt::Check { else_body, .. } => self.statement_uses(else_body),
                Stmt::While { body, .. }
                | Stmt::RepeatUntil { body, .. }
                | Stmt::ForEach { body, .. }
                | Stmt::ForTo { body, .. } => self.statement_uses(body),
                Stmt::Case { arms, default, .. } => {
                    for (_, body) in arms {
                        self.statement_uses(body);
                    }
                    if let Some(default) = default {
                        self.statement_uses(default);
                    }
                }
                _ => {}
            }
        }
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
                // An annotation type composes through neither `like`
                // nor `implementing` (clause 35.2: it cannot be
                // inherited from — E0320).
                DeclarationKind::Annotation { .. } => false,
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
    declarations: &[Declaration],
    kinds: &BTreeMap<&str, DeclarationKind>,
    static_members: &BTreeMap<&str, StaticNames>,
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
    // An annotation body's fields are implicitly `const` (clause 35.2):
    // the generated-field prefix rules that assume instance generation
    // do not apply; the annotation-specific restrictions are this
    // module's `check_annotation_body`.
    let annotation_body = matches!(declaration.kind, DeclarationKind::Annotation { .. });
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
        check_field_prefixes(field, annotation_body, diagnostics);
        if !annotation_body {
            check_unit_instance_static_field(field, kinds, diagnostics);
            check_keyed_list_key(&field.ty, declarations, diagnostics);
        }
        if let Some(init) = &field.init {
            check_expr_static_accesses(init, static_members, diagnostics);
        }
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
///   field without `!`, `const` or `static` has its value produced by
///   `gen`, so a written initial value would never survive. The
///   diagnostic points at the init expression. A `~` field is generated
///   too, so the same rule covers it. A `static` field is exempt: its
///   one shared value is created at the declaration (clause 6.9.3).
/// - A `static const` field requires that init value (`E0212`, clause
///   6.9.4): a const field is initialised once, and for a static field
///   that one initialisation is the declaration's.
/// - A static field is neither physical nor range-generated (`E0305`,
///   clause 6.9.3): there is one shared value, not one per instance to
///   pack or generate.
fn check_field_prefixes(field: &Field, annotation_body: bool, diagnostics: &mut Vec<Diagnostic>) {
    // In an annotation body the field is implicitly const (clause
    // 35.2): an init value is legal, and the static-field rules fire
    // from `check_annotation_body` instead — only the `!`/`~` syntax
    // conflict stays here.
    if annotation_body {
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
        return;
    }
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
        && !field.is_static
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
    if field.is_static && field.is_const && field.init.is_none() {
        diagnostics.push(Diagnostic::new(
            "E0212",
            format!(
                "static const field '{}' requires an initial value (clause 6.9.4)",
                field.name
            ),
            field.name_span.0,
            field.name_span.1,
        ));
    }
    if field.is_static && (field.physical || field.range_gen) {
        diagnostics.push(Diagnostic::new(
            "E0305",
            format!(
                "static field '{}' cannot be physical or generated (clause 6.9.3)",
                field.name
            ),
            field.name_span.0,
            field.name_span.1,
        ));
    }
}

/// A static field is not a unit instance and not a list of unit
/// instances (clauses 6.9.3, 7.2.2, 7.2.4): a unit instance belongs to
/// its parent instance, while a static field's one occurrence belongs
/// to the type and exists with zero instances (`E0306`).
fn check_unit_instance_static_field(
    field: &Field,
    kinds: &BTreeMap<&str, DeclarationKind>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !field.is_static {
        return;
    }
    if let Some(unit_name) = unit_instance_type(&field.ty, kinds) {
        diagnostics.push(Diagnostic::new(
            "E0306",
            format!(
                "static field '{}' of unit-instance type '{}' is not allowed (clause 6.9.3; 7.2.2, 7.2.4)",
                field.name, unit_name
            ),
            field.name_span.0,
            field.name_span.1,
        ));
    }
}

/// The unit-instance type a type expression denotes, if any: a named
/// `unit` (or a builtin unit such as `sys`), through a when-qualified
/// spelling, or as a list element.
fn unit_instance_type(ty: &TypeExpr, kinds: &BTreeMap<&str, DeclarationKind>) -> Option<String> {
    match ty {
        TypeExpr::Named { name, .. }
            if kinds.get(name.as_str()) == Some(&DeclarationKind::Unit)
                || crate::builtins::is_builtin_unit(name) =>
        {
            Some(name.clone())
        }
        TypeExpr::WhenQualified { base, .. } => unit_instance_type(base, kinds),
        TypeExpr::List { element, .. } => unit_instance_type(element, kinds),
        _ => None,
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
    static_members: &BTreeMap<&str, StaticNames>,
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
        // T2.2: a static method has no instance — `me`/`self` and the
        // generation/check/message actions are not usable in it — and
        // every `Type::member` access is validated (T2.1).
        if method.is_static {
            check_static_context_rules(
                &method.body,
                &format!("static method '{}'", method.name),
                diagnostics,
            );
        }
        check_body_static_accesses(&method.body, static_members, diagnostics);
    });
}

/// The static-context restrictions (T2.2, T2.3): `me`/`self` (E0215)
/// and the generation/check/message actions (E0216) are not usable
/// where there is no instance — a static method, or the static `on`
/// forms of clause 11.2. `context` names the enclosing static context
/// for the messages.
fn check_static_context_rules(
    statements: &[Stmt],
    context: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // `gen` and `check that` are statements; the rest are routine
    // calls found in the expression walk below.
    check_static_context_statements(statements, context, diagnostics);
    for_each_stmt_exprs(statements, &mut |expr| match expr {
        Expr::Ident { name, span } if name == "me" || name == "self" => {
            diagnostics.push(Diagnostic::new(
                "E0215",
                format!(
                    "'me' cannot be used in a static context — {context} has no instance (clause 17.2.2)"
                ),
                span.0,
                span.1,
            ));
        }
        Expr::Call { callee, .. } => {
            if let Expr::Ident { name, span } = callee.as_ref() {
                let action = match name.as_str() {
                    "dut_error" => Some(("dut_error", "16.3.3")),
                    "dut_errorf" => Some(("dut_errorf", "16.3.4")),
                    "message" => Some(("message", "24.4.1")),
                    "messagef" => Some(("messagef", "24.4.1")),
                    _ => None,
                };
                if let Some((action, clause)) = action {
                    diagnostics.push(Diagnostic::new(
                        "E0216",
                        format!(
                            "'{action}' is not allowed in a static context — {context} (clause {clause})"
                        ),
                        span.0,
                        span.1,
                    ));
                }
            }
        }
        _ => {}
    });
}

/// The statement-shaped forbidden actions of a static context, at every
/// nesting level: `gen` (clause 10.6.2) and `check that` (clause
/// 16.3.2.1).
fn check_static_context_statements(
    statements: &[Stmt],
    context: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for statement in statements {
        let forbidden = match statement {
            Stmt::Gen { .. } => Some(("gen", "10.6.2")),
            Stmt::Check { .. } => Some(("check that", "16.3.2.1")),
            _ => None,
        };
        if let Some((action, clause)) = forbidden {
            let span = statement.span();
            diagnostics.push(Diagnostic::new(
                "E0216",
                format!(
                    "'{action}' is not allowed in a static context — {context} (clause {clause})"
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
                check_static_context_statements(then_body, context, diagnostics);
                if let Some(else_body) = else_body {
                    check_static_context_statements(else_body, context, diagnostics);
                }
            }
            Stmt::Check { else_body, .. } => {
                check_static_context_statements(else_body, context, diagnostics);
            }
            Stmt::While { body, .. }
            | Stmt::RepeatUntil { body, .. }
            | Stmt::ForEach { body, .. }
            | Stmt::ForTo { body, .. } => {
                check_static_context_statements(body, context, diagnostics);
            }
            Stmt::Case { arms, default, .. } => {
                for (_, body) in arms {
                    check_static_context_statements(body, context, diagnostics);
                }
                if let Some(default) = default {
                    check_static_context_statements(default, context, diagnostics);
                }
            }
            _ => {}
        }
    }
}

/// Validate every `Type::member` access the statement list evaluates
/// (T2.1/T2.2/T2.3): the member must be a static member declared in
/// exactly `Type` (E0307). One pass: every expression the statements
/// reach is visited exactly once.
fn check_body_static_accesses(
    statements: &[Stmt],
    static_members: &BTreeMap<&str, StaticNames>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for_each_stmt_exprs(statements, &mut |expr| {
        if let Expr::StaticAccess {
            type_name,
            member,
            span,
            ..
        } = expr
        {
            check_static_access(type_name, member, *span, static_members, diagnostics);
        }
    });
}

/// Validate the `Type::member` accesses of one expression.
fn check_expr_static_accesses(
    expr: &Expr,
    static_members: &BTreeMap<&str, StaticNames>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for_each_sub_expr(expr, &mut |expr| {
        if let Expr::StaticAccess {
            type_name,
            member,
            span,
            ..
        } = expr
        {
            check_static_access(type_name, member, *span, static_members, diagnostics);
        }
    });
}

/// `Type::member` (clause 4.17.4): the member must be a static member
/// declared in exactly `Type` — not a `like` parent's, not an instance
/// member (E0307). A type with no static-member namespace entry in this
/// file is not checked here: the unknown-type diagnostic is the
/// named-type check's business, and an undeclared target would make
/// every spelling a false positive.
fn check_static_access(
    type_name: &str,
    member: &str,
    span: crate::ast::Span,
    static_members: &BTreeMap<&str, StaticNames>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(names) = static_members.get(type_name) else {
        return;
    };
    if !names.contains(member) {
        diagnostics.push(Diagnostic::new(
            "E0307",
            format!(
                "'{type_name}::{member}': '{member}' is not a static member declared in '{type_name}' (clause 4.17.4)"
            ),
            span.0,
            span.1,
        ));
    }
}

/// A static member belongs to the type itself; a `when` subtype is a
/// view of instances, not a type of its own, so the static member forms
/// are not legal inside a `when` block (clauses 6.9.3, 17.2.2,
/// 11.1.4.1, 11.2). Codes per the migration plan's allocation: the
/// field/method/`on` forms share **`E0304`** ("static member in `when`
/// subtype" — T2.2's and T2.3's `on` forms reuse it), and a static
/// event has the dedicated **`E0310`**. One diagnostic per offending
/// member.
fn check_static_members_in_whens(declaration: &Declaration, diagnostics: &mut Vec<Diagnostic>) {
    fn walk(members: &[Member], in_when: bool, diagnostics: &mut Vec<Diagnostic>) {
        for member in members {
            match member {
                Member::Field(field) if in_when && field.is_static => {
                    diagnostics.push(Diagnostic::new(
                        "E0304",
                        format!(
                            "static field '{}' in when subtype (clause 6.9.3)",
                            field.name
                        ),
                        field.name_span.0,
                        field.name_span.1,
                    ));
                }
                Member::Method(method) if in_when && method.is_static => {
                    diagnostics.push(Diagnostic::new(
                        "E0304",
                        format!(
                            "static method '{}' in when subtype (clause 17.2.2)",
                            method.name
                        ),
                        method.name_span.0,
                        method.name_span.1,
                    ));
                }
                Member::Event(event) if in_when && event.is_static => {
                    diagnostics.push(Diagnostic::new(
                        "E0310",
                        format!(
                            "static event '{}' in when subtype (clause 11.1.4.1)",
                            event.name
                        ),
                        event.name_span.0,
                        event.name_span.1,
                    ));
                }
                Member::On(on) if in_when && (on.is_static || on.type_qualified.is_some()) => {
                    diagnostics.push(Diagnostic::new(
                        "E0304",
                        "static 'on' block in when subtype (clause 11.2)".to_owned(),
                        on.span.0,
                        on.span.1,
                    ));
                }
                Member::When(inner) => walk(&inner.members, true, diagnostics),
                _ => {}
            }
        }
    }
    walk(&declaration.members, false, diagnostics);
}

/// A static method can be extended (`is also`/`is first`/`is only`)
/// only in the struct that declares it — the root-most type of the
/// `like` chain that declares the method without an extension modifier
/// (clause 17.2.4). An extension in a derived struct is reported
/// (`E0309`) once per offending block.
fn check_static_method_extensions(
    declaration: &Declaration,
    declarations: &[Declaration],
    diagnostics: &mut Vec<Diagnostic>,
) {
    for member in &declaration.members {
        let Member::Method(method) = member else {
            continue;
        };
        if !method.is_static || method.modifier.is_none() {
            continue;
        }
        let chain = like_chain(declarations, &declaration.name);
        let declares_static = |ty: &str| {
            declarations
                .iter()
                .filter(|d| d.name == ty && !is_global_pseudo(d))
                .any(|d| {
                    d.members.iter().any(|member| {
                        matches!(
                            member,
                            Member::Method(m)
                                if m.is_static && m.name == method.name && m.modifier.is_none()
                        )
                    })
                })
        };
        let declaring = chain.into_iter().find(|ty| declares_static(ty));
        if let Some(owner) = declaring
            && owner != declaration.name.as_str()
        {
            diagnostics.push(Diagnostic::new(
                "E0309",
                format!(
                    "static method '{}' can only be extended in the struct that declares it ('{}') (clause 17.2.4)",
                    method.name, owner
                ),
                method.name_span.0,
                method.name_span.1,
            ));
        }
    }
}

/// The parser's file-level `define`/`import` pseudo-declaration: scope,
/// not an `extend global` member source (user `extend global` is
/// rejected at parse, E0116).
fn is_global_pseudo(declaration: &Declaration) -> bool {
    declaration.kind == DeclarationKind::Extend
        && declaration.name == "global"
        && declaration.like.is_none()
}

/// The `like` chain of a type: `[type, parent, grandparent, ...]`
/// reversed to root-most first, cycle-safe (a cycle is elaboration's
/// E0301, not this walk's business).
fn like_chain<'a>(declarations: &'a [Declaration], type_name: &'a str) -> Vec<&'a str> {
    let mut chain = vec![type_name];
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    seen.insert(type_name);
    let mut cursor = type_name;
    while let Some(base) = declarations
        .iter()
        .find(|d| d.name == cursor && d.kind != DeclarationKind::Extend)
        .and_then(|d| d.like.as_deref())
    {
        if !seen.insert(base) {
            break;
        }
        chain.push(base);
        cursor = base;
    }
    chain.reverse();
    chain
}

/// The static-member namespace one `Type::member` access resolves
/// against (T2.1–T2.3): per type name, the static fields, methods and
/// events that the type's base declaration and its `extend` blocks
/// declare — the extend target is the declaring type. Only top-level
/// members count: static members inside `when` blocks are rejected
/// separately (E0304/E0310) and never join the namespace.
#[derive(Default)]
struct StaticNames<'a> {
    fields: BTreeSet<&'a str>,
    methods: BTreeSet<&'a str>,
    events: BTreeSet<&'a str>,
}

impl<'a> StaticNames<'a> {
    fn contains(&self, name: &str) -> bool {
        self.fields.contains(name) || self.methods.contains(name) || self.events.contains(name)
    }
}

fn static_member_names<'a>(declarations: &'a [Declaration]) -> BTreeMap<&'a str, StaticNames<'a>> {
    let mut names: BTreeMap<&'a str, StaticNames<'a>> = BTreeMap::new();
    for declaration in declarations {
        if is_global_pseudo(declaration) {
            continue;
        }
        let entry = names.entry(declaration.name.as_str()).or_default();
        for member in &declaration.members {
            match member {
                Member::Field(field) if field.is_static => {
                    entry.fields.insert(field.name.as_str());
                }
                Member::Method(method) if method.is_static => {
                    entry.methods.insert(method.name.as_str());
                }
                Member::Event(event) if event.is_static => {
                    entry.events.insert(event.name.as_str());
                }
                _ => {}
            }
        }
    }
    names
}

/// The keyed-list key restrictions (clause 5.2.11): the key may not be
/// a static field and its type may not be a list or set; a
/// struct-typed key may contain static fields. The key expression
/// names a field of the element type (`list (key:it.field) of T`) —
/// resolved against T's declaration and its extend blocks; a
/// whole-element key (`key:it`) is a struct key and stays legal. This
/// subset's type grammar has no set type spelling, so the list shape
/// is the one this check can see.
fn check_keyed_list_key(
    ty: &TypeExpr,
    declarations: &[Declaration],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let TypeExpr::List {
        element,
        key: Some(key),
        ..
    } = ty
    else {
        return;
    };
    let Some(field_name) = keyed_list_key_field(key) else {
        return;
    };
    let TypeExpr::Named {
        name: element_name, ..
    } = element.as_ref()
    else {
        return;
    };
    let element_field = declarations
        .iter()
        .filter(|d| d.name == element_name.as_str())
        .flat_map(|d| {
            d.members.iter().filter_map(|member| match member {
                Member::Field(field) => Some(field),
                _ => None,
            })
        })
        .find(|field| field.name == field_name);
    let Some(element_field) = element_field else {
        return;
    };
    if element_field.is_static {
        let span = key.span();
        diagnostics.push(Diagnostic::new(
            "E0218",
            format!(
                "keyed list key cannot be a static field: '{element_name}.{}' (clause 5.2.11)",
                element_field.name
            ),
            span.0,
            span.1,
        ));
    } else if matches!(element_field.ty, TypeExpr::List { .. }) {
        let span = key.span();
        diagnostics.push(Diagnostic::new(
            "E0218",
            format!(
                "keyed list key '{}.{}' is of list type; the key type must not be a list or set (clause 5.2.11)",
                element_name, element_field.name
            ),
            span.0,
            span.1,
        ));
    }
}

/// The field name a keyed-list key expression denotes: the last segment
/// of an `it`-rooted member path (`it.k` → `k`). A bare `it` (the
/// whole element as the key) names no field.
fn keyed_list_key_field(key: &Expr) -> Option<&str> {
    fn rooted_at_it(expr: &Expr) -> bool {
        match expr {
            Expr::Ident { name, .. } => name == "it",
            Expr::MemberAccess { object, .. } => rooted_at_it(object),
            _ => false,
        }
    }
    match key {
        Expr::MemberAccess { object, name, .. } if rooted_at_it(object) => Some(name),
        _ => None,
    }
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

/// Visit every expression reachable from the statement, nested bodies
/// included (`if`/`while`/`for`/`case`/`check`-else). Each expression is
/// visited exactly once. The temporal expressions of `wait`/`sync` are
/// not walked (their engine is a separate stage); an `evaluate` block's
/// statements are.
fn for_each_stmt_exprs<'a>(statements: &'a [Stmt], f: &mut impl FnMut(&'a Expr)) {
    for statement in statements {
        for_each_stmt_expr(statement, f);
    }
}

fn for_each_stmt_expr<'a>(statement: &'a Stmt, f: &mut impl FnMut(&'a Expr)) {
    match statement {
        Stmt::VarDecl {
            init: Some(init), ..
        } => for_each_sub_expr(init, f),
        Stmt::Assign { target, value, .. } => {
            for_each_sub_expr(target, f);
            for_each_sub_expr(value, f);
        }
        Stmt::ExprStmt { expr, .. } => for_each_sub_expr(expr, f),
        Stmt::Return {
            value: Some(value), ..
        } => for_each_sub_expr(value, f),
        Stmt::If {
            cond,
            then_body,
            else_body,
            ..
        } => {
            for_each_sub_expr(cond, f);
            for_each_stmt_exprs(then_body, f);
            if let Some(else_body) = else_body {
                for_each_stmt_exprs(else_body, f);
            }
        }
        Stmt::While { cond, body, .. } => {
            for_each_sub_expr(cond, f);
            for_each_stmt_exprs(body, f);
        }
        Stmt::RepeatUntil { body, cond, .. } => {
            for_each_stmt_exprs(body, f);
            for_each_sub_expr(cond, f);
        }
        Stmt::Print { exprs, .. } => {
            for expr in exprs {
                for_each_sub_expr(expr, f);
            }
        }
        Stmt::Gen { expr, .. } => for_each_sub_expr(expr, f),
        Stmt::Check {
            condition,
            else_body,
            ..
        } => {
            for_each_sub_expr(condition, f);
            for_each_stmt_exprs(else_body, f);
        }
        Stmt::ForEach {
            using, list, body, ..
        } => {
            if let Some(using) = using {
                for_each_sub_expr(using, f);
            }
            for_each_sub_expr(list, f);
            for_each_stmt_exprs(body, f);
        }
        Stmt::ForTo { from, to, body, .. } => {
            for_each_sub_expr(from, f);
            for_each_sub_expr(to, f);
            for_each_stmt_exprs(body, f);
        }
        Stmt::Wait { cycles, .. } => for_each_sub_expr(cycles, f),
        Stmt::Emit { target, .. } => for_each_sub_expr(target, f),
        Stmt::Start { call, .. } | Stmt::Compute { call, .. } => for_each_sub_expr(call, f),
        Stmt::Case {
            scrutinee,
            arms,
            default,
            ..
        } => {
            for_each_sub_expr(scrutinee, f);
            for (alternatives, body) in arms {
                for alternative in alternatives {
                    for_each_sub_expr(alternative, f);
                }
                for_each_stmt_exprs(body, f);
            }
            if let Some(default) = default {
                for_each_stmt_exprs(default, f);
            }
        }
        // `break`/`continue`/`stop_run`/`do` carry no expression; the
        // temporal forms of `wait`/`sync` are the temporal engine's
        // grammar, not walked here.
        _ => {}
    }
}

/// Visit the expression and every sub-expression exactly once — the
/// full recursive walk over the expression grammar. Type positions
/// embedded in expressions (`new`'s type, casts, `typeof` arguments'
/// type grammar) contribute their expression children only where the
/// grammar stores one (`typeof`/`typeof_item` arguments).
fn for_each_sub_expr<'a>(expr: &'a Expr, f: &mut impl FnMut(&'a Expr)) {
    f(expr);
    match expr {
        Expr::IntLiteral { .. }
        | Expr::RealLiteral { .. }
        | Expr::BitLiteral { .. }
        | Expr::BoolLiteral { .. }
        | Expr::StringLiteral { .. }
        | Expr::Ident { .. }
        | Expr::HdlPath { .. }
        | Expr::SetOfValues { .. }
        // The `Type` of a static access is a name, not an expression.
        | Expr::StaticAccess { .. }
        // An annotation type reference is a name (clause 35.4); its
        // member accesses walk through the MemberAccess arm.
        | Expr::AnnotationTypeRef { .. }
        // A set literal's bounds enumerate, they are not walked here.
        | Expr::SetLiteral { .. } => {}
        Expr::MemberAccess { object, .. } => for_each_sub_expr(object, f),
        Expr::Call { callee, args, .. } => {
            for_each_sub_expr(callee, f);
            for arg in args {
                for_each_sub_expr(arg, f);
            }
        }
        Expr::Index { base, index, .. } => {
            for_each_sub_expr(base, f);
            for_each_sub_expr(index, f);
        }
        Expr::Binary { lhs, rhs, .. } => {
            for_each_sub_expr(lhs, f);
            for_each_sub_expr(rhs, f);
        }
        Expr::Unary { expr, .. } | Expr::Paren { expr, .. } => for_each_sub_expr(expr, f),
        Expr::Ternary {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            for_each_sub_expr(cond, f);
            for_each_sub_expr(then_expr, f);
            for_each_sub_expr(else_expr, f);
        }
        Expr::New { inits, .. } => for_each_stmt_exprs(inits, f),
        Expr::Select { arms, .. } => {
            for arm in arms {
                for_each_sub_expr(&arm.weight, f);
            }
        }
        Expr::CastAs { expr, .. } => for_each_sub_expr(expr, f),
        Expr::BitSlice { base, high, low, .. } => {
            for_each_sub_expr(base, f);
            for_each_sub_expr(high, f);
            for_each_sub_expr(low, f);
        }
        Expr::InSet { element, set, .. } => {
            for_each_sub_expr(element, f);
            match set {
                crate::ast::InSetExpr::Items(items) => {
                    for item in items {
                        for_each_sub_expr(item, f);
                    }
                }
                crate::ast::InSetExpr::Range(low, high) => {
                    for_each_sub_expr(low, f);
                    for_each_sub_expr(high, f);
                }
            }
        }
        Expr::IsA { expr, .. } => for_each_sub_expr(expr, f),
        Expr::InTable { tuple, rows, .. } => {
            for element in tuple {
                for_each_sub_expr(element, f);
            }
            for row in rows {
                for element in row {
                    match element {
                        crate::ast::TableElem::Scalar(expr) => for_each_sub_expr(expr, f),
                        crate::ast::TableElem::Set(crate::ast::InSetExpr::Items(items)) => {
                            for item in items {
                                for_each_sub_expr(item, f);
                            }
                        }
                        crate::ast::TableElem::Set(crate::ast::InSetExpr::Range(low, high)) => {
                            for_each_sub_expr(low, f);
                            for_each_sub_expr(high, f);
                        }
                        crate::ast::TableElem::TypeSet { .. } => {}
                    }
                }
            }
        }
        Expr::Evaluate { body, .. } => for_each_stmt_exprs(body, f),
        Expr::QualifiedField { base, .. } => for_each_sub_expr(base, f),
        Expr::ListConcat { items, .. } => {
            for item in items {
                for_each_sub_expr(item, f);
            }
        }
    }
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
    // Unit attribute constraints (7.3.4 `pack_options()`, 7.3.5
    // `per_inst_cover()`) compare against the predefined attribute
    // values — the `packing` type and its enumerands, and the
    // per_inst_cover settings — which are not fields of the enclosing
    // declaration. The recording pass (elab) reads them as written.
    let attribute_constraint =
        crate::interp::expr_calls_any(&constraint.expr, &["pack_options", "per_inst_cover"]);
    for (name, span) in roots {
        if name == "me" || name == "self" {
            continue;
        }
        if attribute_constraint && matches!(name.as_str(), "packing" | "DEFAULT" | "YES" | "NO") {
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
        // `Type::member` names a static member of a type — a legal
        // constraint reference, not an unknown root (clause 4.17.4).
        Expr::StaticAccess { .. } => {}
        // `@name.member` names an annotation type's API method (clause
        // 35.4) — likewise a legal reference, not an unknown root.
        Expr::AnnotationTypeRef { .. } => {}
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
        // A set literal's bounds are ordinary sub-expressions.
        Expr::SetLiteral {
            items: crate::ast::InSetExpr::Items(items),
            ..
        } => {
            for item in items {
                collect_root_identifiers(item, roots);
            }
        }
        Expr::SetLiteral {
            items: crate::ast::InSetExpr::Range(low, high),
            ..
        } => {
            collect_root_identifiers(low, roots);
            collect_root_identifiers(high, roots);
        }
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
