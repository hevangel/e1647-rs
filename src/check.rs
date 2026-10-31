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
//!
//! Fields, methods and constraints are collected through `when` subtype
//! members as well. A `keep` constraint sees every field of the enclosing
//! declaration, including fields of sibling `when` blocks — subtype-aware
//! scoping is elaboration and a later milestone.

use crate::ast::{
    Declaration, DeclarationKind, Expr, Field, KeepConstraint, Member, MethodDecl, Stmt, TypeExpr,
};
use crate::diagnostic::Diagnostic;
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
    for declaration in declarations {
        check_extend_target(declaration, &declared, &mut diagnostics);
        check_like_target(declaration, &declared, &mut diagnostics);
        if let Some(underlying) = &declaration.underlying {
            check_named_type(underlying, &declared, &mut diagnostics);
        }
        check_fields(declaration, &declared, &mut diagnostics);
        check_enumerals(declaration, &mut diagnostics);
        check_methods(declaration, &declared, &mut diagnostics);
        for_each_on(&declaration.members, &mut |on| {
            let mut seen: BTreeSet<&str> = BTreeSet::new();
            check_statements(
                &on.body,
                &mut seen,
                &declared,
                &on.path.join("."),
                &mut diagnostics,
            );
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
    for_each_field(&declaration.members, &mut |field| {
        check_named_type(&field.ty, declared, diagnostics);
        if let TypeExpr::Scalar {
            base: crate::ast::ScalarBase::Uint | crate::ast::ScalarBase::Int,
            width: Some(width),
            ..
        } = &field.ty
        {
            check_width(width, diagnostics);
        }
    });
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
    diagnostics: &mut Vec<Diagnostic>,
) {
    for_each_method(&declaration.members, &mut |method| {
        // Parameters and local variables share one namespace in this subset;
        // block scoping is a later milestone.
        let mut seen: BTreeSet<&str> = BTreeSet::new();
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
            }
        }
        if let Some(return_type) = &method.return_type {
            check_named_type(return_type, declared, diagnostics);
        }
        check_statements(&method.body, &mut seen, declared, &method.name, diagnostics);
    });
}

/// Walk a statement list. `seen` accumulates local variable names across
/// nested `if` bodies — they are treated as one flat scope for now.
fn check_statements<'a>(
    statements: &'a [Stmt],
    seen: &mut BTreeSet<&'a str>,
    declared: &BTreeSet<&str>,
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
                    check_named_type(ty, declared, diagnostics);
                }
            }
            Stmt::If {
                then_body,
                else_body,
                ..
            } => {
                check_statements(then_body, seen, declared, method_name, diagnostics);
                if let Some(else_body) = else_body {
                    check_statements(else_body, seen, declared, method_name, diagnostics);
                }
            }
            Stmt::Check { else_body, .. } => {
                check_statements(else_body, seen, declared, method_name, diagnostics);
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
            | Stmt::Break { .. }
            | Stmt::Continue { .. }
            | Stmt::Case { .. }
            | Stmt::Wait { .. }
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

fn check_named_type(ty: &TypeExpr, declared: &BTreeSet<&str>, diagnostics: &mut Vec<Diagnostic>) {
    match ty {
        TypeExpr::Named { name, span } => {
            if !declared.contains(name.as_str()) {
                diagnostics.push(Diagnostic::new(
                    "E0203",
                    format!("unknown type '{name}'"),
                    span.0,
                    span.1,
                ));
            }
        }
        TypeExpr::List { element, .. } => check_named_type(element, declared, diagnostics),
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
        // `new` creates an instance; its type names no field and its
        // init assignments run against the fresh instance.
        Expr::New { inits, .. } => {
            for statement in inits {
                collect_with_init_roots(statement, roots);
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
