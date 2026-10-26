//! Minimal well-formedness checks over the parsed declarations.
//!
//! This is **not** full symbol/type checking: there is no type system, no
//! cross-file scope, and no elaboration yet. Checks, all single-file scope:
//!
//! - `E0201` two base declarations (`struct`/`unit`) share a name
//! - `E0202` a declaration declares the same field name twice
//! - `E0203` a field type or an `extend` target names a type not declared
//!   in this file (external declarations are a later milestone)
//! - `E0204` a `keep` constraint in a `struct`/`unit` references an
//!   identifier that is not one of its fields (base fields of `extend`
//!   targets are unknown here, so `extend` declarations are skipped)
//! - `E0205` a `uint`/`int` width literal is zero

use crate::ast::{Declaration, DeclarationKind, Expr, TypeExpr};
use crate::diagnostic::Diagnostic;
use std::collections::BTreeSet;

pub fn check_declarations(declarations: &[Declaration]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    // Single-file scope: base `struct`/`unit` declarations are the visible
    // type universe. Multiple `extend` blocks of the same target are legal,
    // so duplicates are checked only among base declarations.
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
    for declaration in declarations {
        check_extend_target(declaration, &declared, &mut diagnostics);
        check_like_target(declaration, &declared, &mut diagnostics);
        check_fields(declaration, &declared, &mut diagnostics);
        if declaration.kind != DeclarationKind::Extend {
            let field_names: BTreeSet<&str> = declaration
                .members
                .iter()
                .filter_map(|member| match member {
                    crate::ast::Member::Field(field) => Some(field.name.as_str()),
                    crate::ast::Member::Constraint(_) => None,
                })
                .collect();
            for member in &declaration.members {
                if let crate::ast::Member::Constraint(constraint) = member {
                    check_constraint_fields(constraint, &field_names, &mut diagnostics);
                }
            }
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
    if !declared.contains(declaration.name.as_str()) {
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
    if !declared.contains(like.as_str()) {
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
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for member in &declaration.members {
        let crate::ast::Member::Field(field) = member else {
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
        if let TypeExpr::Named { name, span } = &field.ty
            && !declared.contains(name.as_str())
        {
            diagnostics.push(Diagnostic::new(
                "E0203",
                format!("unknown type '{name}'"),
                span.0,
                span.1,
            ));
        }
        if let TypeExpr::Scalar {
            base: crate::ast::ScalarBase::Uint | crate::ast::ScalarBase::Int,
            width: Some(width),
            ..
        } = &field.ty
        {
            check_width(width, diagnostics);
        }
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

/// Report root identifiers of the constraint expression that are not fields
/// of the enclosing declaration.
fn check_constraint_fields(
    constraint: &crate::ast::KeepConstraint,
    field_names: &BTreeSet<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut roots = Vec::new();
    collect_root_identifiers(&constraint.expr, &mut roots);
    for (name, span) in roots {
        if !field_names.contains(name.as_str()) {
            diagnostics.push(Diagnostic::new(
                "E0204",
                format!("unknown field '{name}' in keep constraint"),
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
        Expr::Binary { lhs, rhs, .. } => {
            collect_root_identifiers(lhs, roots);
            collect_root_identifiers(rhs, roots);
        }
        Expr::Unary { expr, .. } | Expr::Paren { expr, .. } => {
            collect_root_identifiers(expr, roots)
        }
        Expr::IntLiteral { .. }
        | Expr::RealLiteral { .. }
        | Expr::BitLiteral { .. }
        | Expr::BoolLiteral { .. }
        | Expr::StringLiteral { .. } => {}
    }
}
