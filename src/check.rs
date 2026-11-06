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
//! - `E0219` reserved (the plan allocates it to the interfaces stage:
//!   conflicting inherited interface signatures)
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
        check_extend_target(declaration, &declared, &mut diagnostics);
        check_like_target(declaration, &declared, &mut diagnostics);
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
        if declaration.kind != DeclarationKind::Extend {
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
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(like) = &declaration.like else {
        return;
    };
    let like_span = declaration.like_span.unwrap_or(declaration.name_span);
    if !declared.contains(like.as_str()) && !crate::builtins::is_builtin_base(like) {
        diagnostics.push(Diagnostic::new(
            "E0203",
            format!("unknown type '{like}' in like clause"),
            like_span.0,
            like_span.1,
        ));
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
        check_unit_instance_static_field(field, kinds, diagnostics);
        check_keyed_list_key(&field.ty, declarations, diagnostics);
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
/// resolution, 5.9) and the enclosing declaration's own fields
/// (`me.field`).
struct StatementScope<'a> {
    declared: &'a BTreeSet<&'a str>,
    struct_fields: &'a BTreeMap<&'a str, Vec<(&'a str, &'a TypeExpr)>>,
    own_fields: &'a BTreeMap<&'a str, &'a TypeExpr>,
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
            Stmt::Assign { .. }
            | Stmt::ExprStmt { .. }
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
        | Expr::StaticAccess { .. } => {}
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
        // `Type::member` names a static member of a type — a legal
        // constraint reference, not an unknown root (clause 4.17.4).
        Expr::StaticAccess { .. } => {}
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
