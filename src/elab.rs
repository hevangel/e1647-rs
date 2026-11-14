//! Member composition for the declared subset: `like` inheritance chains
//! and `extend` blocks are folded into one composed member list per
//! `struct`/`unit`, and `type` declarations pass through with their
//! enumerals.
//!
//! This is **not** full elaboration: `when` subtypes are carried, not
//! resolved; there is no type checking and no execution semantics.
//! Composition is order-independent on declarations (e allows an `extend`
//! before its target) and deterministic: base-most `like` members first,
//! then the declaring members, then each `extend` block in source order.
//! The built-in `sys` unit is extended without being declared and is
//! synthesized from its `extend` blocks alone.
//!
//! ## Diagnostics (on top of the parser's)
//!
//! - `E0224` invalid coverage option/range parameters or per-unit-instance scope
//!   (clauses 14.2–14.3), checked after member composition
//! - `E0301` `like` inheritance cycle (reported once per type)
//! - `E0302` the same field name is contributed by two different
//!   declarations or extend blocks (duplicates inside one declaration are
//!   the parser's `E0202`; reported once per name)
//! - `E0303` a method is redefined without `is also` / `is first` /
//!   `is only` (reported once per name)
//!
//! ## Interface attachment design (clause 6.13, 6.3 — the migration's
//! T3.1–T3.4)
//!
//! Interfaces attach to composed types through the same member-composition
//! model as everything else, without joining `like` chains:
//!
//! - An `interface` declaration composes to its own [`ElaboratedType`]
//!   (kind `Interface`) whose `methods` are its transitive signatures —
//!   the `like` bases' members folded first (in base order, recursively),
//!   then its own; a repeated method name keeps its first entry. Interfaces
//!   never enter the struct `like`-composition tables, so a struct cannot
//!   inherit one as a base (the checker rejects that and `extend I`).
//! - A struct/unit's `implementing` clause is collected by the same
//!   `compose` walk that folds `like` chains and extends: the like-chain
//!   bases' interface sets come first, then the declaration's own, then
//!   each extend block's, deduped in written order. The composed set is
//!   recorded on the elaborated type; synthesized `when`-subtype views
//!   clone it (subtypes implement their base's interfaces). Assignment/
//!   cast conformance checks read this set: a struct value cast to an
//!   interface is kept when the set contains the interface (or an
//!   interface that inherits it) and becomes NULL otherwise.
//! - Dynamic dispatch needs no new machinery: an interface-typed value IS
//!   the implementing instance (`Value::Object`), so a method call through
//!   it resolves on the instance's actual type. The only addition is the
//!   missing-method diagnosis: when the call's object expression is
//!   interface-typed and the interface (transitively) declares the method
//!   the instance's type does not provide, the interpreter reports `E0414`
//!   instead of the plain not-found error.
//!
//! The interface-conflict diagnostics (E0219, E0311) and the assignment
//! conformance check (E0312) run in `check.rs` — the well-formedness pass,
//! which the fixture contract sees; this module records what the checks
//! and the interpreter consume.
//! - `E0127` a `template extend`'s parameter count differs from the
//!   template declaration's (8.6; reported once per extension; the
//!   pre-plan E0318 allocation is retired by T4.4)
//!
//! The template bounding diagnostics of the 2019 migration — `E0313` a
//! value actual not a constant of the parameter's scalar type, `E0314`
//! an instance violating the template's `conditions`, `E0315` a type
//! actual not in the parameter's category (clauses 8.5.2, 8.5.3) — are
//! raised from the well-formedness pass ([`crate::check`]) like
//! E0311/E0312 before them: the fixture harness and the CLI's `parse`
//! command stop at parse + check, so the diagnostics must be visible
//! there. The monomorphizer reads the same declarations and binding
//! rules; a non-matching `template extend` simply does not apply (8.6).
//!
//! The elaboration-range static-member codes of the 2019 migration
//! (`E0304`–`E0309`, `E0310`) are raised from the well-formedness pass
//! ([`crate::check`]) in this stage: the fixture harness and the CLI's
//! `parse` command stop at parse + check, so the diagnostics must be
//! visible there; the codes keep their plan allocation. The static
//! member registries this module produces ([`Elaborated::static_fields`]
//! and siblings) are the interpreter's static-storage keyspace.

use crate::ast::{
    Declaration, DeclarationKind, Enumeral, EventDecl, Expr, Field, KeepConstraint, Member,
    MethodDecl, Stmt, TypeExpr, WhenDecl,
};
use crate::diagnostic::Diagnostic;
use std::collections::{BTreeMap, BTreeSet};

/// A compile-time constant (clause 8.5.2: a value parameter's actual is
/// a constant expression; 8.5.3: a condition is a constant expression).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Const {
    Int(i128),
    Real(f64),
    Bool(bool),
    Str(String),
}

impl Const {
    /// The constant as a literal expression — the normalized form a
    /// value parameter's actual substitutes as and canonical instance
    /// names render.
    pub(crate) fn as_expr(&self) -> Expr {
        let zero = crate::lexer::Position {
            byte: 0,
            line: 0,
            column: 0,
        };
        match self {
            Const::Int(value) => Expr::IntLiteral {
                text: value.to_string(),
                span: (zero, zero),
            },
            Const::Real(value) => Expr::RealLiteral {
                text: format!("{value}"),
                span: (zero, zero),
            },
            Const::Bool(value) => Expr::BoolLiteral {
                value: *value,
                span: (zero, zero),
            },
            Const::Str(value) => Expr::StringLiteral {
                value: value.clone(),
                span: (zero, zero),
            },
        }
    }
}

/// Evaluate a constant expression (clauses 8.5.2, 8.5.3): literals, the
/// arithmetic / comparison / logical operators, `?:`, and the names a
/// constant may reference — an enclosing value parameter's constant
/// (`values`) or a `define` constant (`defines`, recursively,
/// depth-capped). `None` for anything else (the condition or actual is
/// then not constant).
pub(crate) fn eval_const_expr(
    expr: &Expr,
    values: &BTreeMap<String, Expr>,
    defines: &BTreeMap<String, Expr>,
    depth: usize,
) -> Option<Const> {
    if depth > 16 {
        return None;
    }
    match expr {
        Expr::IntLiteral { text, .. } => crate::interp::parse_int_literal(text).map(Const::Int),
        Expr::RealLiteral { text, .. } => text.parse::<f64>().ok().map(Const::Real),
        Expr::BoolLiteral { value, .. } => Some(Const::Bool(*value)),
        Expr::StringLiteral { value, .. } => Some(Const::Str(value.clone())),
        Expr::Formal { name, .. } => eval_const_expr(values.get(name)?, values, defines, depth + 1),
        Expr::Ident { name, .. } => eval_const_expr(defines.get(name)?, values, defines, depth + 1),
        Expr::Paren { expr, .. } => eval_const_expr(expr, values, defines, depth + 1),
        Expr::Unary { op, expr, .. } => {
            let inner = eval_const_expr(expr, values, defines, depth + 1)?;
            match (op, inner) {
                (crate::ast::UnaryOp::Negate, Const::Int(v)) => Some(Const::Int(-v)),
                (crate::ast::UnaryOp::Negate, Const::Real(v)) => Some(Const::Real(-v)),
                (crate::ast::UnaryOp::Plus, inner) => Some(inner),
                (crate::ast::UnaryOp::Not, Const::Bool(v)) => Some(Const::Bool(!v)),
                (crate::ast::UnaryOp::BitNot, Const::Int(v)) => Some(Const::Int(!v)),
                _ => None,
            }
        }
        Expr::Binary { op, lhs, rhs, .. } => {
            let l = eval_const_expr(lhs, values, defines, depth + 1)?;
            let r = eval_const_expr(rhs, values, defines, depth + 1)?;
            eval_const_binary(*op, l, r)
        }
        Expr::Ternary {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            let branch = match eval_const_expr(cond, values, defines, depth + 1)? {
                Const::Bool(true) => then_expr,
                Const::Bool(false) => else_expr,
                _ => return None,
            };
            eval_const_expr(branch, values, defines, depth + 1)
        }
        _ => None,
    }
}

/// One binary operator of the constant evaluator.
fn eval_const_binary(op: crate::ast::BinaryOp, l: Const, r: Const) -> Option<Const> {
    use crate::ast::BinaryOp as Op;
    // The logical and comparison operators accept booleans or integers;
    // the arithmetic ones integers (reals where both are real).
    if let (Const::Bool(a), Const::Bool(b)) = (&l, &r) {
        return match op {
            Op::LogicalAnd => Some(Const::Bool(*a && *b)),
            Op::LogicalOr => Some(Const::Bool(*a || *b)),
            Op::Imply => Some(Const::Bool(!*a || *b)),
            Op::Eq => Some(Const::Bool(a == b)),
            Op::NotEq => Some(Const::Bool(a != b)),
            _ => None,
        };
    }
    if let (Const::Str(a), Const::Str(b)) = (&l, &r) {
        return match op {
            Op::Eq => Some(Const::Bool(a == b)),
            Op::NotEq => Some(Const::Bool(a != b)),
            _ => None,
        };
    }
    // Numeric level: mixed int/real promotes to real.
    match (l, r) {
        (Const::Int(a), Const::Int(b)) => match op {
            Op::Add => Some(Const::Int(a.checked_add(b)?)),
            Op::Sub => Some(Const::Int(a.checked_sub(b)?)),
            Op::Mul => Some(Const::Int(a.checked_mul(b)?)),
            Op::Div => Some(Const::Int(a.checked_div(b)?)),
            Op::Mod => Some(Const::Int(a.checked_rem(b)?)),
            Op::ShiftLeft => Some(Const::Int(a.checked_shl(b.try_into().ok()?)?)),
            Op::ShiftRight => Some(Const::Int(a.checked_shr(b.try_into().ok()?)?)),
            Op::BitAnd => Some(Const::Int(a & b)),
            Op::BitOr => Some(Const::Int(a | b)),
            Op::BitXor => Some(Const::Int(a ^ b)),
            Op::Eq => Some(Const::Bool(a == b)),
            Op::NotEq => Some(Const::Bool(a != b)),
            Op::Lt => Some(Const::Bool(a < b)),
            Op::Le => Some(Const::Bool(a <= b)),
            Op::Gt => Some(Const::Bool(a > b)),
            Op::Ge => Some(Const::Bool(a >= b)),
            Op::LogicalAnd => Some(Const::Bool(a != 0 && b != 0)),
            Op::LogicalOr => Some(Const::Bool(a != 0 || b != 0)),
            _ => None,
        },
        (l, r) => {
            let a = match l {
                Const::Int(v) => v as f64,
                Const::Real(v) => v,
                _ => return None,
            };
            let b = match r {
                Const::Int(v) => v as f64,
                Const::Real(v) => v,
                _ => return None,
            };
            match op {
                Op::Add => Some(Const::Real(a + b)),
                Op::Sub => Some(Const::Real(a - b)),
                Op::Mul => Some(Const::Real(a * b)),
                Op::Div => Some(Const::Real(a / b)),
                Op::Eq => Some(Const::Bool(a == b)),
                Op::NotEq => Some(Const::Bool(a != b)),
                Op::Lt => Some(Const::Bool(a < b)),
                Op::Le => Some(Const::Bool(a <= b)),
                Op::Gt => Some(Const::Bool(a > b)),
                Op::Ge => Some(Const::Bool(a >= b)),
                _ => None,
            }
        }
    }
}

/// The file's `define` constants (literal values only, like the
/// interpreter's table): the name space a constant expression may
/// reference.
pub(crate) fn collect_defines(declarations: &[Declaration]) -> BTreeMap<String, Expr> {
    let mut defines = BTreeMap::new();
    for declaration in declarations {
        for member in &declaration.members {
            if let Member::Define {
                name,
                value: Some(value),
                ..
            } = member
            {
                defines.insert(name.clone(), value.clone());
            }
        }
    }
    defines
}

/// The binding of one template instance: the full positional actuals
/// (written actuals with the trailing defaults filled and value actuals
/// normalized to their evaluated constants), the type-formal map, and
/// the value-formal map of evaluated constants (clauses 8.3, 8.5.4).
pub(crate) struct TemplateBinding {
    pub actuals: Vec<TypeExpr>,
    pub types: BTreeMap<String, TypeExpr>,
    pub values: BTreeMap<String, Expr>,
}

/// Bind a template's parameters against written actuals (8.3's
/// positional rule with 8.5.4's trailing defaults). A parameter with
/// neither an actual nor a default leaves the instance unsatisfied
/// (`None`); a default referencing earlier parameters substitutes
/// through the bindings built so far, a value default evaluating to its
/// constant. Every template reference the defaults uncover joins the
/// queue.
pub(crate) fn bind_template_actuals(
    declarations: &[Declaration],
    template: &Declaration,
    args: &[TypeExpr],
    defines: &BTreeMap<String, Expr>,
    queue: &mut Vec<(String, Vec<TypeExpr>)>,
) -> Option<TemplateBinding> {
    let mut binding = TemplateBinding {
        actuals: Vec::new(),
        types: BTreeMap::new(),
        values: BTreeMap::new(),
    };
    for (index, param) in template.template_params.iter().enumerate() {
        if let Some(actual) = args.get(index) {
            // 8.5.2: a value parameter's actual is a constant
            // expression — an argument naming a `define` constant
            // (spelled as an identifier, the type grammar's shape) is
            // the constant's reference, not a type.
            let actual = match (&param.kind, actual) {
                (crate::ast::TemplateParamKind::Value { .. }, TypeExpr::Named { name, span })
                    if defines.contains_key(name) =>
                {
                    TypeExpr::Value {
                        expr: Box::new(Expr::Ident {
                            name: name.clone(),
                            span: *span,
                        }),
                        span: *span,
                    }
                }
                _ => actual.clone(),
            };
            record_actual(param, &actual, &mut binding, defines);
        } else {
            let default = param.default.as_ref()?;
            // The default substitutes through the bindings built so far
            // (an 8.5.4 default may reference earlier parameters of
            // either kind).
            let mut ctx = Subst {
                declarations,
                types: binding.types.clone(),
                values: binding.values.clone(),
                defines,
                queue,
            };
            match default {
                crate::ast::TemplateDefault::Type(default_type) => {
                    let substituted = substitute_type(default_type, &mut ctx);
                    binding.types = ctx.types.clone();
                    binding.values = ctx.values.clone();
                    record_actual(param, &substituted, &mut binding, defines);
                }
                crate::ast::TemplateDefault::Value(default_expr) => {
                    let mut substituted = default_expr.clone();
                    substitute_expr(&mut substituted, &mut ctx);
                    let substituted = match eval_const_expr(&substituted, &ctx.values, defines, 0) {
                        Some(constant) => constant.as_expr(),
                        None => substituted,
                    };
                    ctx.values.insert(param.name.clone(), substituted.clone());
                    let actual = TypeExpr::Value {
                        expr: Box::new(substituted),
                        span: default_expr.span(),
                    };
                    binding.types = ctx.types;
                    binding.values = ctx.values;
                    binding.types.insert(param.name.clone(), actual.clone());
                    binding.actuals.push(actual);
                }
            }
        }
    }
    if binding.actuals.is_empty() {
        return None;
    }
    Some(binding)
}

/// Record one bound actual for its parameter: the type map gains every
/// actual (a value actual rides as a [`TypeExpr::Value`]), the value map
/// only a value parameter's evaluated constant.
fn record_actual(
    param: &crate::ast::TemplateParam,
    actual: &TypeExpr,
    binding: &mut TemplateBinding,
    defines: &BTreeMap<String, Expr>,
) {
    // A value actual normalizes to its evaluated constant — the form
    // the canonical instance name renders (8.3: same actuals, one
    // instance) and the value map carries.
    let actual = match (&param.kind, actual) {
        (crate::ast::TemplateParamKind::Value { .. }, TypeExpr::Value { expr, span }) => {
            match eval_const_expr(expr, &binding.values, defines, 0) {
                Some(constant) => TypeExpr::Value {
                    expr: Box::new(constant.as_expr()),
                    span: *span,
                },
                None => actual.clone(),
            }
        }
        _ => actual.clone(),
    };
    binding.types.insert(param.name.clone(), actual.clone());
    if let crate::ast::TemplateParamKind::Value { .. } = param.kind
        && let TypeExpr::Value { expr, .. } = &actual
        && let Some(constant) = eval_const_expr(expr, &binding.values, defines, 0)
    {
        binding
            .values
            .insert(param.name.clone(), constant.as_expr());
    }
    binding.actuals.push(actual);
}

/// Whether a type actual complies with a type parameter's category
/// (clause 8.5.3's bounding criteria, 8.2's NOTE 3 application rule).
/// The recognized categories are the extractable ones — `type` (any),
/// `scalar`, `numeric`, `enumerated`/`enum`, `bool`, `string`,
/// `struct`, `unit`; any other spelling reads as a specific type name
/// the actual must render to exactly (8.2's specific-type form; the
/// full category table of 8.1.2 is an image in the local redline copy).
pub(crate) fn actual_in_category(
    actual: &TypeExpr,
    category: &str,
    declarations: &[Declaration],
) -> bool {
    match category {
        "type" => true,
        "scalar" => actual_is_scalar(actual, declarations),
        "numeric" => actual_is_numeric(actual, declarations),
        "enumerated" | "enum" => actual_is_enumerated(actual, declarations),
        "bool" => actual_is_base(actual, ScalarBaseKind::Bool, declarations),
        "string" => actual_is_base(actual, ScalarBaseKind::String, declarations),
        "struct" => actual_is_kind(actual, true, declarations),
        "unit" => actual_is_kind(actual, false, declarations),
        specific => actual.render() == specific,
    }
}

/// The predefined scalar bases a category check reads through.
enum ScalarBaseKind {
    Bool,
    String,
    /// The integral and real bases (the `numeric` category).
    Numeric,
}

/// Category compliance over declarations: a predefined scalar base of
/// the kind, an inline enumeral list, or a declared `type` whose
/// underlying representation matches (5.1's reading: scalar types are
/// the numeric, Boolean and enumerated types).
fn actual_is_scalar(actual: &TypeExpr, declarations: &[Declaration]) -> bool {
    match actual {
        TypeExpr::Scalar { base, .. } => !matches!(base, crate::ast::ScalarBase::String),
        TypeExpr::InlineEnum { .. } => true,
        TypeExpr::Named { name, .. } => {
            named_type_underlying(name, declarations).is_some_and(|underlying| {
                matches!(
                    underlying,
                    TypeExpr::Scalar { .. } | TypeExpr::InlineEnum { .. }
                )
            })
        }
        _ => false,
    }
}

/// The `numeric` category: the integral and real bases, or a declared
/// type over them.
fn actual_is_numeric(actual: &TypeExpr, declarations: &[Declaration]) -> bool {
    actual_is_base(actual, ScalarBaseKind::Numeric, declarations)
}

/// The `enumerated` category: an inline enumeral list or a declared
/// type with enumerals (8.2.1's enumerated-type reading).
fn actual_is_enumerated(actual: &TypeExpr, declarations: &[Declaration]) -> bool {
    match actual {
        TypeExpr::InlineEnum { .. } => true,
        TypeExpr::Named { name, .. } => declarations.iter().any(|declaration| {
            declaration.kind == DeclarationKind::Type
                && declaration.name == *name
                && declaration
                    .members
                    .iter()
                    .any(|member| matches!(member, Member::Enumeral(_)))
        }),
        _ => false,
    }
}

/// One scalar-base category check (`bool`, `string`, `numeric`).
fn actual_is_base(actual: &TypeExpr, kind: ScalarBaseKind, declarations: &[Declaration]) -> bool {
    let base_matches = |base: &crate::ast::ScalarBase| match kind {
        ScalarBaseKind::Bool => *base == crate::ast::ScalarBase::Bool,
        ScalarBaseKind::String => *base == crate::ast::ScalarBase::String,
        ScalarBaseKind::Numeric => !matches!(
            base,
            crate::ast::ScalarBase::Bool | crate::ast::ScalarBase::String
        ),
    };
    match actual {
        TypeExpr::Scalar { base, .. } => base_matches(base),
        TypeExpr::Named { name, .. } => named_type_underlying(name, declarations).is_some_and(
            |underlying| matches!(&underlying, TypeExpr::Scalar { base, .. } if base_matches(base)),
        ),
        _ => false,
    }
}

/// The `struct`/`unit` categories: a declared struct (or unit) type.
fn actual_is_kind(actual: &TypeExpr, is_struct: bool, declarations: &[Declaration]) -> bool {
    let TypeExpr::Named { name, .. } = actual else {
        return false;
    };
    declarations.iter().any(|declaration| {
        declaration.name == *name
            && if is_struct {
                matches!(
                    declaration.kind,
                    DeclarationKind::Struct | DeclarationKind::Unit
                )
            } else {
                declaration.kind == DeclarationKind::Unit
            }
    })
}

/// The underlying representation of a declared scalar/list `type` (the
/// record-only clause 5 form), by name.
fn named_type_underlying<'d>(name: &str, declarations: &'d [Declaration]) -> Option<&'d TypeExpr> {
    declarations
        .iter()
        .find(|declaration| declaration.kind == DeclarationKind::Type && declaration.name == name)
        .and_then(|declaration| declaration.underlying.as_ref())
}

/// One elaborated type: a `struct`/`unit` with its composed member lists,
/// or a `type` declaration with its enumerals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElaboratedType {
    pub kind: DeclarationKind,
    pub name: String,
    /// The `like` base, as written.
    pub base: Option<String>,
    pub fields: Vec<Field>,
    pub constraints: Vec<KeepConstraint>,
    pub methods: Vec<MethodDecl>,
    /// `when` subtype members, ancestor types first.
    pub whens: Vec<WhenDecl>,
    /// `on` struct members (clause 11.2), base types first, then extends
    /// in declaration order — the accumulated action blocks per event.
    pub ons: Vec<crate::ast::OnDecl>,
    /// Coverage groups (clause 14), recorded not collected.
    pub covers: Vec<crate::ast::CoverGroupDecl>,
    /// `expect`/`assume` rules (clause 11.4), base types first, then
    /// extends in declaration order — monitored over the run log by the
    /// interpreter.
    pub expects: Vec<crate::ast::ExpectDecl>,
    /// For a synthesized when-subtype view (6.7): every discriminating
    /// (field id, modifier value) pair — `[("kind", "BUSY")]` for
    /// `when BUSY'kind`, `[("kind", "LO"), ("sel", "HI")]` for the
    /// chained `when LO'kind HI'sel` or the equivalent two-level
    /// when-in-when composition. Empty for regular types.
    pub when_of: Vec<(String, String)>,
    /// `event` members (11.1.3) with their triggers as written — the
    /// simulator-side triggers (`rise/fall/change(HDL expression) @sim`,
    /// 11.1.4.2) drive the interpreter's sampling. Every instance owns
    /// its events (11.1.2); the interpreter instantiates per instance.
    pub events: Vec<crate::ast::EventDecl>,
    /// `type name: <type>;` — the underlying representation, recorded
    /// only (clause 5); distinct-type semantics are a later milestone.
    pub underlying: Option<crate::ast::TypeExpr>,
    pub enumerals: Vec<Enumeral>,
    /// `simple_port` members with the HDL location resolved from their
    /// `keep name.hdl_path() == "path"` constraints.
    pub ports: Vec<PortInfo>,
    /// Unit attribute constraints (clause 7.3.4 `pack_options()`, 7.3.5
    /// `per_inst_cover()`), recorded like the port HDL paths:
    /// (field, attribute, value as written) — `("u1", "pack_options",
    /// "packing.low_big_endian")`. The packing and
    /// per-instance-coverage engines they configure are later
    /// milestones; the recording keeps the constraint out of gen and
    /// makes the setting visible.
    pub unit_attributes: Vec<(String, String, String)>,
    /// `implementing I1, I2, ...` composed per type (clause 6.3): the
    /// like-chain bases' interfaces first, then the declaration's own,
    /// then each extend block's, deduped in written order. Synthesized
    /// when-subtype views inherit the base's set — subtypes implement
    /// their base's interfaces. The conformance checks (E0312 in
    /// `check.rs`, the casts and `is a` in `interp.rs`) and the interface
    /// conflict check (E0311) read this set.
    pub implements: Vec<String>,
    /// For an `interface` declaration (clause 6.13): the inherited
    /// interfaces in written order. The composed `methods` list already
    /// includes the transitive bases' signatures.
    pub interface_bases: Vec<String>,
    pub span: crate::ast::Span,
}

/// A `simple_port` member resolved for simulation: the port name and the
/// HDL location its `hdl_path()` constraint declared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortInfo {
    pub name: String,
    pub hdl_path: Option<String>,
    /// An event port (9.5): carries events, not data — its `$` event is
    /// triggerable by sampling the bound HDL object per its `edge()`
    /// attribute (9.7.3), not only by `emit port$`.
    pub is_event: bool,
}

/// One `sequence NAME using item = T, created_driver = D, created_kind
/// = K;` declaration, recovered from the synthesized define markers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceInfo {
    pub name: String,
    pub item: String,
    pub driver: String,
    pub kind: String,
}

/// Outcome of elaborating one file's declarations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Elaborated {
    /// In declaration order; `extend` blocks are folded into their targets
    /// and do not appear separately.
    pub types: Vec<ElaboratedType>,
    /// `define` constants by name (literal values only).
    pub defines: BTreeMap<String, crate::ast::Expr>,
    /// Sequence declarations recovered from the parser's synthesized
    /// markers.
    pub sequences: Vec<SequenceInfo>,
    pub diagnostics: Vec<Diagnostic>,
    /// The static members (2019 static-members stage: clauses 6.9.3
    /// fields, 17.2.2 methods, 11.1.4.1 events, 11.2 `on` blocks) by the
    /// canonical name of the type that declares them — the base
    /// declaration's name, or the extend target for an `extend T {
    /// static ... }` member, so an extension adds to the same store the
    /// declaration owns. Template-extension members are not folded in:
    /// they apply per instance (8.2/8.3-NOTE-3), which a per-type
    /// singleton contradicts (a documented gap). `when` blocks never
    /// contribute — a static member there is rejected before elaboration
    /// (E0304/E0310 at the well-formedness pass). The interpreter keys
    /// its static store and its `Type::member` resolution by these names;
    /// the well-formedness pass builds its own view of the same
    /// namespace straight from the declarations.
    pub static_fields: BTreeMap<String, Vec<Field>>,
    pub static_methods: BTreeMap<String, Vec<MethodDecl>>,
    pub static_events: BTreeMap<String, Vec<EventDecl>>,
    /// Every static-context `on` block: `static on <event>` keyed by its
    /// declaring type, and `on Type::event` keyed by the type that
    /// declares the block (the interpreter matches the latter by the
    /// block's `type_qualified` target instead).
    pub static_ons: BTreeMap<String, Vec<crate::ast::OnDecl>>,
    /// The annotation types (clause 35.2) by name, with their
    /// `repeatable` flag — the interpreter's `new` rejection (E0402)
    /// and the clause 35.4 API dispatch read this.
    pub annotation_types: BTreeMap<String, bool>,
    /// The attached annotations (clause 35.3) by annotation type name,
    /// in attachment order — the clause 35.4 `get_all_annotations()`
    /// returns these. Extend blocks fold into their target types, but
    /// their members' attachments record here all the same.
    pub attached_annotations: BTreeMap<String, Vec<AttachedAnnotation>>,
}

/// One attached annotation (clause 35.3) recorded for the clause 35.4
/// API: the annotation type's name, the attachment's `(attribute,
/// value-as-written)` pairs and the attachment's span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachedAnnotation {
    pub type_name: String,
    pub attrs: Vec<(String, crate::ast::Expr)>,
    pub span: crate::ast::Span,
}

/// Collect one declaration's attachments — the declaration's own and
/// every member's, `when` bodies included — into the registry.
fn collect_declaration_annotations(
    declaration: &Declaration,
    registry: &mut BTreeMap<String, Vec<AttachedAnnotation>>,
) {
    for annotation in &declaration.annotations {
        registry
            .entry(annotation.name.clone())
            .or_default()
            .push(AttachedAnnotation {
                type_name: annotation.name.clone(),
                attrs: annotation.attrs.clone(),
                span: annotation.span,
            });
    }
    collect_member_annotations(&declaration.members, registry);
}

fn collect_member_annotations(
    members: &[Member],
    registry: &mut BTreeMap<String, Vec<AttachedAnnotation>>,
) {
    for member in members {
        if let Member::When(when) = member {
            collect_member_annotations(&when.members, registry);
        }
        let annotations: &[crate::ast::Annotation] = match member {
            Member::Field(field) => &field.annotations,
            Member::Constraint(constraint) => &constraint.annotations,
            Member::Method(method) => &method.annotations,
            Member::Event(event) => &event.annotations,
            Member::On(on) => &on.annotations,
            Member::Cover(cover) => &cover.annotations,
            Member::Expect(expect) => &expect.annotations,
            Member::Port(port) => &port.annotations,
            Member::Macro(macro_decl) => &macro_decl.annotations,
            _ => continue,
        };
        for annotation in annotations {
            registry
                .entry(annotation.name.clone())
                .or_default()
                .push(AttachedAnnotation {
                    type_name: annotation.name.clone(),
                    attrs: annotation.attrs.clone(),
                    span: annotation.span,
                });
        }
    }
}

/// One type's members, tagged with the declaration or extend block that
/// contributed them (its index in the declaration slice). Events and
/// `on` blocks carry the block index too — the static-member
/// registries need the declaring type.
#[derive(Debug, Default, Clone)]
struct Composed<'a> {
    fields: Vec<(&'a Field, usize)>,
    constraints: Vec<&'a KeepConstraint>,
    methods: Vec<(&'a MethodDecl, usize)>,
    whens: Vec<&'a WhenDecl>,
    ons: Vec<(&'a crate::ast::OnDecl, usize)>,
    covers: Vec<&'a crate::ast::CoverGroupDecl>,
    expects: Vec<&'a crate::ast::ExpectDecl>,
    events: Vec<(&'a crate::ast::EventDecl, usize)>,
    ports: Vec<(&'a crate::ast::PortDecl, usize)>,
    /// The `implementing` clause composed across the like chain and the
    /// extend blocks (clause 6.3), deduped in written order — as
    /// written: a template's parameterised entries substitute at
    /// instantiation (8.7).
    implements: Vec<crate::ast::TypeExpr>,
}

impl<'a> Composed<'a> {
    /// Move `other`'s members in front of this set (base-first
    /// composition order — the `any_unit` hooks precede the unit's own).
    fn prepend(&mut self, other: Composed<'a>) {
        let Composed {
            fields,
            constraints,
            methods,
            whens,
            ons,
            covers,
            expects,
            events,
            ports,
            implements,
        } = other;
        let own = std::mem::take(self);
        *self = Composed {
            fields: fields.into_iter().chain(own.fields).collect(),
            constraints: constraints.into_iter().chain(own.constraints).collect(),
            methods: methods.into_iter().chain(own.methods).collect(),
            whens: whens.into_iter().chain(own.whens).collect(),
            ons: ons.into_iter().chain(own.ons).collect(),
            covers: covers.into_iter().chain(own.covers).collect(),
            expects: expects.into_iter().chain(own.expects).collect(),
            events: events.into_iter().chain(own.events).collect(),
            ports: ports.into_iter().chain(own.ports).collect(),
            implements: implements.into_iter().chain(own.implements).collect(),
        };
    }
}

struct Elaborator<'a> {
    declarations: &'a [Declaration],
    base_by_name: BTreeMap<&'a str, usize>,
    extends_by_target: BTreeMap<&'a str, Vec<usize>>,
    diagnostics: Vec<Diagnostic>,
    reported_fields: BTreeSet<&'a str>,
    reported_methods: BTreeSet<&'a str>,
    reported_cycles: BTreeSet<&'a str>,
}

/// Fold `like` chains and `extend` blocks into per-type member lists.
pub fn elaborate(declarations: &[Declaration]) -> Elaborated {
    let mut base_by_name: BTreeMap<&str, usize> = BTreeMap::new();
    let mut extends_by_target: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    let mut template_extends_by_target: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    let mut annotation_types: BTreeMap<String, bool> = BTreeMap::new();
    for (index, declaration) in declarations.iter().enumerate() {
        match declaration.kind {
            DeclarationKind::Extend => {
                // A template extension (8.2) applies per instance at
                // monomorphization (8.3's NOTE 3) — the template's own
                // composed members stay extension-free, and a
                // non-matching instance does not get the extension.
                if declaration.template_extend.is_some() {
                    template_extends_by_target
                        .entry(declaration.name.as_str())
                        .or_default()
                        .push(index);
                } else {
                    extends_by_target
                        .entry(declaration.name.as_str())
                        .or_default()
                        .push(index);
                }
            }
            DeclarationKind::Struct
            | DeclarationKind::Unit
            | DeclarationKind::Type
            | DeclarationKind::NumericType => {
                base_by_name.insert(declaration.name.as_str(), index);
            }
            // An interface is not a `like`-composable struct base (the
            // checker rejects `struct S like I` and `extend I`); it
            // composes through its own arm below.
            DeclarationKind::Interface => {}
            // An annotation type (clause 35.2) joins no composition: it
            // cannot be extended or inherited from (E0320). Its name and
            // `repeatable` flag are recorded for the clause 35.4 API.
            DeclarationKind::Annotation { repeatable } => {
                annotation_types.insert(declaration.name.clone(), repeatable);
            }
        }
    }
    let mut elaborator = Elaborator {
        declarations,
        base_by_name,
        extends_by_target,
        diagnostics: Vec::new(),
        reported_fields: BTreeSet::new(),
        reported_methods: BTreeSet::new(),
        reported_cycles: BTreeSet::new(),
    };
    let mut types = Vec::new();
    // The `define` constants (literal values only): the name space a
    // constant expression (8.5.2's actuals, 8.5.3's conditions and
    // 8.5.4's defaults) may reference, shared by every stage below.
    let defines = collect_defines(declarations);
    // The monomorphization worklist (8.3), seeded by the composed
    // declarations' `implementing` entries and grown by the
    // substitution passes.
    let mut queue: Vec<(String, Vec<crate::ast::TypeExpr>)> = Vec::new();
    // `extend any_unit { ... }` members (24.9.2.14's extendable message
    // hooks): composed once and injected into every unit type, base-first.
    let any_unit = elaborator.compose_any_unit();
    for declaration in declarations {
        match declaration.kind {
            DeclarationKind::Struct | DeclarationKind::Unit => {
                let mut stack = Vec::new();
                let mut composed = elaborator.compose(declaration.name.as_str(), &mut stack);
                if declaration.kind == DeclarationKind::Unit {
                    composed.prepend(any_unit.clone());
                    // Field duplicates between the injected any_unit
                    // members and the type's own are cross-set pairs the
                    // per-block checks above cannot see (the reported
                    // sets keep the within-set pairs from re-reporting).
                    elaborator.check_duplicate_fields(declaration.name.as_str(), &composed);
                }
                let port_paths = elaborator.extract_port_paths(declaration);
                let unit_attributes = elaborator.extract_unit_attributes(declaration);
                // The `implementing` entries render to the canonical
                // names the conformance checks and the interpreter read;
                // a parameterised entry's instance joins the worklist.
                let implements =
                    render_implements(&composed.implements, declarations, &defines, &mut queue);
                types.push(ElaboratedType {
                    kind: declaration.kind,
                    name: declaration.name.clone(),
                    base: declaration.like.clone(),
                    fields: composed
                        .fields
                        .into_iter()
                        .map(|(field, _)| field.clone())
                        .collect(),
                    constraints: composed.constraints.into_iter().cloned().collect(),
                    methods: composed
                        .methods
                        .into_iter()
                        .map(|(method, _)| method.clone())
                        .collect(),
                    whens: composed.whens.into_iter().cloned().collect(),
                    ons: composed.ons.into_iter().map(|(on, _)| on.clone()).collect(),
                    covers: composed.covers.into_iter().cloned().collect(),
                    expects: composed.expects.into_iter().cloned().collect(),
                    when_of: Vec::new(),
                    events: composed
                        .events
                        .into_iter()
                        .map(|(event, _)| event.clone())
                        .collect(),
                    underlying: None,
                    enumerals: Vec::new(),
                    ports: composed
                        .ports
                        .into_iter()
                        .map(|(port, _)| PortInfo {
                            name: port.name.clone(),
                            hdl_path: port_paths.get(port.name.as_str()).cloned(),
                            is_event: matches!(
                                &port.element,
                                crate::ast::TypeExpr::Named { name, .. } if name == "event"
                            ),
                        })
                        .collect(),
                    unit_attributes,
                    implements,
                    interface_bases: Vec::new(),
                    span: declaration.span,
                });
            }
            DeclarationKind::Type => types.push(ElaboratedType {
                kind: DeclarationKind::Type,
                name: declaration.name.clone(),
                base: None,
                fields: Vec::new(),
                constraints: Vec::new(),
                methods: Vec::new(),
                whens: Vec::new(),
                ons: Vec::new(),
                covers: Vec::new(),
                expects: Vec::new(),
                when_of: Vec::new(),
                events: Vec::new(),
                underlying: declaration.underlying.clone(),
                ports: Vec::new(),
                unit_attributes: Vec::new(),
                implements: Vec::new(),
                interface_bases: Vec::new(),
                enumerals: declaration
                    .members
                    .iter()
                    .filter_map(|member| match member {
                        Member::Enumeral(enumeral) => Some(enumeral.clone()),
                        _ => None,
                    })
                    .collect(),
                span: declaration.span,
            }),
            // Clause 8.4: a template numeric_type records its
            // declaration view (the impl-struct reference as written);
            // instances materialize at monomorphization with their
            // parameters substituted.
            DeclarationKind::NumericType => types.push(ElaboratedType {
                kind: DeclarationKind::NumericType,
                name: declaration.name.clone(),
                base: None,
                fields: Vec::new(),
                constraints: Vec::new(),
                methods: Vec::new(),
                whens: Vec::new(),
                ons: Vec::new(),
                covers: Vec::new(),
                expects: Vec::new(),
                when_of: Vec::new(),
                events: Vec::new(),
                underlying: declaration.underlying.clone(),
                ports: Vec::new(),
                unit_attributes: Vec::new(),
                implements: Vec::new(),
                interface_bases: Vec::new(),
                enumerals: Vec::new(),
                span: declaration.span,
            }),
            // Clause 6.13: an interface composes to its transitive
            // signatures — the `like` bases' members folded first (in
            // base order, recursively), then its own; a repeated method
            // name keeps its first entry (exact duplicates collapse;
            // conflicting signatures are E0219 in `check.rs`). No
            // `extend` folding — extending an interface is rejected. A
            // parameterised base (`J of <t'type>`, clause 8.3) folds by
            // the template's name in the declaration view; the
            // substituted bases compose in the instance view.
            DeclarationKind::Interface => {
                let mut methods: Vec<MethodDecl> = Vec::new();
                let mut seen: BTreeSet<String> = BTreeSet::new();
                let mut stack: Vec<String> = Vec::new();
                fold_interface_methods(
                    &declaration.name,
                    declarations,
                    &mut stack,
                    &mut seen,
                    &mut methods,
                    &mut elaborator.diagnostics,
                );
                types.push(ElaboratedType {
                    kind: DeclarationKind::Interface,
                    name: declaration.name.clone(),
                    base: None,
                    fields: Vec::new(),
                    constraints: Vec::new(),
                    methods,
                    whens: Vec::new(),
                    ons: Vec::new(),
                    covers: Vec::new(),
                    expects: Vec::new(),
                    when_of: Vec::new(),
                    events: Vec::new(),
                    underlying: None,
                    ports: Vec::new(),
                    unit_attributes: Vec::new(),
                    implements: Vec::new(),
                    interface_bases: declaration
                        .bases
                        .iter()
                        .filter_map(|(base, _)| match base {
                            TypeExpr::Named { name, .. } => Some(name.clone()),
                            _ => None,
                        })
                        .collect(),
                    enumerals: Vec::new(),
                    span: declaration.span,
                });
            }
            // An annotation type produces no composed type view (its
            // members are recorded on the declaration; the clause 35.4
            // API reads the attachment registry). Extends of it are
            // rejected (E0320).
            DeclarationKind::Annotation { .. } => {}
            DeclarationKind::Extend => {}
        }
    }
    elaborator.compose_builtin_units(&mut types, &defines, &mut queue);
    // The attached annotations (clause 35.3), by annotation type name in
    // attachment order — the clause 35.4 `get_all_annotations()` reads
    // this registry. Extend blocks fold into their target types, but
    // their attachments record here all the same.
    let mut attached_annotations: BTreeMap<String, Vec<AttachedAnnotation>> = BTreeMap::new();
    for declaration in declarations {
        collect_declaration_annotations(declaration, &mut attached_annotations);
    }
    // 6.7: synthesize one subtype view per `when` block (`when
    // BUSY'kind { .. }` on type `kind`'s owner): the base's composed
    // members with the when block's members appended, under the
    // `VALUE base` name the field-type spelling uses. Instances typed
    // `VALUE base` (TypeExpr::WhenQualified) carry this view, so the
    // when block's fields, constraints, methods, on blocks and cover
    // groups apply for them.
    //
    // Chained modifiers compose conjunctively (6.7.1: subtypes can be
    // combinations of fields): every modifier names its determinant
    // field, and the view carries all the (field, value) pairs under a
    // name that space-joins the values in modifier order (`V1 V2
    // base`). A `when` member nested inside a when block applies on
    // top of the enclosing view: the outer view is synthesized first
    // and the inner when's modifiers run against the synthesized
    // view's fields, producing a further view named with the combined
    // values. Both levels share one worklist: each synthesized view is
    // queued back, and its own nested whens synthesize on top of it
    // until no new views appear (budget-capped — 200 synthesized
    // views — against pathological nesting).
    let base_count = types.len();
    let mut worklist: Vec<usize> = (0..base_count).collect();
    let mut budget = 200usize;
    while let Some(index) = worklist.pop() {
        let whens: Vec<crate::ast::WhenDecl> = types[index].whens.clone();
        let root_base = when_root_base_name(&types, index);
        for when in whens {
            let Some(subtype) = when_modifier_pairs(&when, &types[index], &root_base, &types)
            else {
                continue;
            };
            if budget == 0 {
                break;
            }
            budget -= 1;
            // The enclosing view's own values (none for a base type)
            // come first, the inner when's value words follow, and the
            // root base type's name closes — the reading order the
            // chained field-type spelling uses (`f: LO HI pkt`). The
            // name words are the written subtype values (`red good` for
            // `when red good packet`), not the resolved determinant
            // values (`TRUE` for the `good` short form).
            let mut values: Vec<String> = types[index]
                .when_of
                .iter()
                .map(|(_, value)| value.clone())
                .collect();
            values.extend(subtype.name_words);
            let base_name = when_root_base_name(&types, index);
            let enclosing_name = types[index].name.clone();
            let mut view = types[index].clone();
            view.name = format!("{} {}", values.join(" "), base_name);
            // The combined view's direct base is the view it applies on
            // top of (`LO HI pkt` → `LO pkt` → `pkt`), so `is a` walks
            // the containment chain.
            view.base = Some(enclosing_name);
            let mut all_pairs = types[index].when_of.clone();
            all_pairs.extend(subtype.pairs);
            view.when_of = all_pairs;
            // The base's other when blocks are alternative subtypes,
            // not refinements of this one — the view composes only the
            // when-in-when members of the block it came from.
            view.whens = Vec::new();
            for member in &when.members {
                match member {
                    crate::ast::Member::Field(field) => view.fields.push(field.clone()),
                    crate::ast::Member::Constraint(constraint) => {
                        view.constraints.push(constraint.clone())
                    }
                    crate::ast::Member::Method(method) => view.methods.push(method.clone()),
                    crate::ast::Member::On(on) => view.ons.push(on.clone()),
                    crate::ast::Member::Cover(cover) => view.covers.push(cover.clone()),
                    crate::ast::Member::Expect(expect) => view.expects.push(expect.clone()),
                    // A when-in-when member rides on the synthesized
                    // view: carry it in the view's whens so the worklist
                    // applies it on top.
                    crate::ast::Member::When(inner_when) => view.whens.push(inner_when.clone()),
                    _ => {}
                }
            }
            types.push(view);
            worklist.push(types.len() - 1);
        }
    }
    // 8.3: monomorphize the template instances the code uses. "A
    // template instance creates a new struct, by substituting an
    // actual parameter for the corresponding template parameter within
    // the template body" — and within the parameterized base, the
    // `implementing` entries and the bodies of any applicable template
    // extensions (8.2, 8.6). Trailing parameters may be omitted when
    // defaults exist (8.5.4); two instances with the same actuals are
    // the same instance — the canonical name renders the
    // defaults-filled actuals, value actuals as their evaluated
    // constants. The worklist seeds from every type's member types,
    // method bodies and `implementing` entries, and grows with the
    // references substitution uncovers.
    for entry in &mut types {
        substitute_entry(entry, declarations, &defines, &mut queue);
    }
    let mut done: BTreeSet<String> = types.iter().map(|t| t.name.clone()).collect();
    let mut reported_extension_arity: BTreeSet<usize> = BTreeSet::new();
    let mut budget = 500;
    while let Some((template_name, args)) = queue.pop() {
        budget -= 1;
        if budget == 0 {
            break;
        }
        let Some(template) = declarations.iter().find(|declaration| {
            declaration.name == template_name
                && !declaration.template_params.is_empty()
                && declaration.template_extend.is_none()
        }) else {
            // Not a template (or the reference targets a template
            // extension's name): mark the written spelling done so the
            // same occurrence does not retry.
            done.insert(crate::ast::template_instance_name(&template_name, &args));
            continue;
        };
        // Bind formals: positional actuals, then the trailing defaults
        // (8.3's trailing-omission rule with 8.5.4's derived defaults).
        // A parameter with neither is not provided for — the instance
        // is skipped, honestly.
        let Some(binding) =
            bind_template_actuals(declarations, template, &args, &defines, &mut queue)
        else {
            continue;
        };
        let canonical = crate::ast::template_instance_name(&template_name, &binding.actuals);
        if done.contains(&canonical) {
            continue;
        }
        done.insert(canonical.clone());
        // 8.3: a template interface's instance is an interface — the
        // substituted bases' transitive signatures folded first (in
        // base order), then its own substituted signatures; a repeated
        // method name keeps its first entry.
        if template.kind == DeclarationKind::Interface {
            let mut methods: Vec<MethodDecl> = Vec::new();
            let mut seen: BTreeSet<String> = BTreeSet::new();
            let mut interface_bases: Vec<String> = Vec::new();
            let mut stack: Vec<String> = Vec::new();
            let mut ctx = Subst {
                declarations,
                types: binding.types.clone(),
                values: binding.values.clone(),
                defines: &defines,
                queue: &mut queue,
            };
            for (base_type, _) in &template.bases {
                let substituted = substitute_type(base_type, &mut ctx);
                let base_name = match &substituted {
                    TypeExpr::Named { name, .. } => Some(name.clone()),
                    // A parameterised base instance carries its
                    // canonical name (enqueued above, materialized by
                    // the worklist).
                    TypeExpr::TemplateRef { .. } => Some(substituted.render()),
                    _ => None,
                };
                let Some(base_name) = base_name else {
                    continue;
                };
                fold_interface_methods(
                    &base_name,
                    declarations,
                    &mut stack,
                    &mut seen,
                    &mut methods,
                    &mut elaborator.diagnostics,
                );
                if !interface_bases.contains(&base_name) {
                    interface_bases.push(base_name);
                }
            }
            for member in &template.members {
                let Member::Method(method) = member else {
                    continue;
                };
                let owned = substitute_method(method.clone(), &mut ctx);
                if seen.insert(owned.name.clone()) {
                    methods.push(owned);
                }
            }
            types.push(ElaboratedType {
                kind: DeclarationKind::Interface,
                name: canonical,
                base: None,
                fields: Vec::new(),
                constraints: Vec::new(),
                methods,
                whens: Vec::new(),
                ons: Vec::new(),
                covers: Vec::new(),
                expects: Vec::new(),
                when_of: Vec::new(),
                events: Vec::new(),
                underlying: None,
                enumerals: Vec::new(),
                ports: Vec::new(),
                unit_attributes: Vec::new(),
                implements: Vec::new(),
                interface_bases,
                span: template.span,
            });
            continue;
        }
        // 8.4: a template numeric_type's instance records the
        // substituted impl-struct reference; the operator machinery is
        // the later numeric_type milestone.
        if template.kind == DeclarationKind::NumericType {
            let mut ctx = Subst {
                declarations,
                types: binding.types.clone(),
                values: binding.values.clone(),
                defines: &defines,
                queue: &mut queue,
            };
            let underlying = template
                .underlying
                .as_ref()
                .map(|underlying| substitute_type(underlying, &mut ctx));
            types.push(ElaboratedType {
                kind: DeclarationKind::NumericType,
                name: canonical,
                base: None,
                fields: Vec::new(),
                constraints: Vec::new(),
                methods: Vec::new(),
                whens: Vec::new(),
                ons: Vec::new(),
                covers: Vec::new(),
                expects: Vec::new(),
                when_of: Vec::new(),
                events: Vec::new(),
                underlying,
                enumerals: Vec::new(),
                ports: Vec::new(),
                unit_attributes: Vec::new(),
                implements: Vec::new(),
                interface_bases: Vec::new(),
                span: template.span,
            });
            continue;
        }
        let mut ctx = Subst {
            declarations,
            types: binding.types.clone(),
            values: binding.values.clone(),
            defines: &defines,
            queue: &mut queue,
        };
        let mut stack = Vec::new();
        let composed = elaborator.compose(template.name.as_str(), &mut stack);
        let mut fields = Vec::new();
        for (field, _) in composed.fields {
            let mut owned = field.clone();
            owned.ty = substitute_type(&owned.ty, &mut ctx);
            fields.push(owned);
        }
        let mut constraints: Vec<KeepConstraint> =
            composed.constraints.into_iter().cloned().collect();
        for constraint in &mut constraints {
            substitute_expr(&mut constraint.expr, &mut ctx);
        }
        let mut methods = Vec::new();
        for (method, _) in composed.methods {
            methods.push(substitute_method(method.clone(), &mut ctx));
        }
        let mut ons: Vec<crate::ast::OnDecl> =
            composed.ons.into_iter().map(|(on, _)| on.clone()).collect();
        for on in &mut ons {
            substitute_statements(&mut on.body, &mut ctx);
        }
        // 8.7 with 8.2.3: the `implementing` entries substitute — a
        // parameterised interface over the template's formals renders
        // to the instance's canonical name (enqueued above,
        // materialized by the worklist); concrete entries pass through.
        let mut implements: Vec<String> = Vec::new();
        for interface in &composed.implements {
            let substituted = substitute_type(interface, &mut ctx);
            let name = substituted.render();
            if !implements.contains(&name) {
                implements.push(name);
            }
        }
        drop(ctx);
        // 8.2 with 8.6: each template extension applies to this
        // instance only if every actual matches the extension's
        // entries — a parameter form binds the actual to the
        // extension's own formal (a category constrains it), a
        // concrete type-name or constant constrains the match — and
        // the extension's `conditions` hold (8.5.3's bounding
        // criteria). A complying extension's members substitute the
        // extension's own formals (its tags may differ from the
        // template's) and append; a non-matching one does not apply
        // at all.
        let extension_indices = template_extends_by_target
            .get(template.name.as_str())
            .cloned()
            .unwrap_or_default();
        for extension_index in extension_indices {
            let extension = &declarations[extension_index];
            let Some(spec) = &extension.template_extend else {
                continue;
            };
            if spec.entries.len() != template.template_params.len() {
                // 8.6: the extension's parameter count must equal the
                // original template declaration's — reported once per
                // extension, and the extension does not apply. The
                // plan's E0127 (the pre-plan E0318 is retired by T4.4).
                if reported_extension_arity.insert(extension_index) {
                    let (start, end) = extension.name_span;
                    elaborator.diagnostics.push(Diagnostic::new(
                        "E0127",
                        "template extend: parameter count mismatch",
                        start,
                        end,
                    ));
                }
                continue;
            }
            let Some((extension_types, extension_values)) = match_template_extend_entries(
                &spec.entries,
                &binding.actuals,
                declarations,
                &defines,
            ) else {
                continue;
            };
            // 8.6 with 8.5.3: the extension's own conditions, over its
            // own tags bound to the instance's actuals, must all hold.
            // A condition that does not evaluate to a constant leaves
            // the match undecided — the extension does not apply.
            let conditions_hold = extension.template_conditions.iter().all(|condition| {
                let mut substituted = condition.clone();
                let mut queue = Vec::new();
                let mut extension_ctx = Subst {
                    declarations,
                    types: extension_types.clone(),
                    values: extension_values.clone(),
                    defines: &defines,
                    queue: &mut queue,
                };
                substitute_expr(&mut substituted, &mut extension_ctx);
                matches!(
                    eval_const_expr(&substituted, &extension_values, &defines, 0),
                    Some(Const::Bool(true))
                )
            });
            if !conditions_hold {
                continue;
            }
            let mut extension_queue = Vec::new();
            let mut extension_ctx = Subst {
                declarations,
                types: extension_types,
                values: extension_values,
                defines: &defines,
                queue: &mut extension_queue,
            };
            for member in &extension.members {
                match member {
                    Member::Field(field) => {
                        let mut owned = field.clone();
                        owned.ty = substitute_type(&owned.ty, &mut extension_ctx);
                        fields.push(owned);
                    }
                    Member::Constraint(constraint) => {
                        let mut owned = constraint.clone();
                        substitute_expr(&mut owned.expr, &mut extension_ctx);
                        constraints.push(owned);
                    }
                    Member::Method(method) => {
                        methods.push(substitute_method(method.clone(), &mut extension_ctx));
                    }
                    Member::On(on) => {
                        let mut owned = on.clone();
                        substitute_statements(&mut owned.body, &mut extension_ctx);
                        ons.push(owned);
                    }
                    Member::Enumeral(_)
                    | Member::Import { .. }
                    | Member::Define { .. }
                    | Member::Macro(_) => {}
                    // A `when` member of an extension is clause 8.6's
                    // template subtype instance; cover groups, expects,
                    // ports and events in an extension body do not
                    // compose yet — documented gaps.
                    _ => {}
                }
            }
            // 8.6: the extension's `implementing` entries add to the
            // instance's composed set (substituted, like the members).
            for interface in &extension.implements {
                let substituted = substitute_type(interface, &mut extension_ctx);
                let name = substituted.render();
                if !implements.contains(&name) {
                    implements.push(name);
                }
            }
            queue.append(&mut extension_queue);
        }
        // The parameterized base substitutes to this view's base name —
        // a plain type's name, or another instance's canonical name
        // (enqueued above, materialized by the worklist).
        let mut base_ctx = Subst {
            declarations,
            types: binding.types.clone(),
            values: binding.values.clone(),
            defines: &defines,
            queue: &mut queue,
        };
        let base = template.like_type.as_ref().map(|like_type| {
            match substitute_type(like_type, &mut base_ctx) {
                crate::ast::TypeExpr::TemplateRef { name, args, .. } => {
                    crate::ast::template_instance_name(&name, &args)
                }
                crate::ast::TypeExpr::Named { name, .. } => name,
                other => other.render(),
            }
        });
        drop(base_ctx);
        types.push(ElaboratedType {
            kind: template.kind,
            name: canonical,
            base,
            fields,
            constraints,
            methods,
            whens: composed.whens.into_iter().cloned().collect(),
            ons,
            covers: composed.covers.into_iter().cloned().collect(),
            expects: composed.expects.into_iter().cloned().collect(),
            when_of: Vec::new(),
            events: Vec::new(),
            underlying: None,
            enumerals: Vec::new(),
            ports: Vec::new(),
            unit_attributes: Vec::new(),
            implements,
            interface_bases: Vec::new(),
            span: template.span,
        });
    }
    // 6.7 with 5.9: two final passes over the composed types.
    // (a) every when-qualified type spelling used in a type position
    //     names a subtype view; a spelling without a when-block view
    //     (`good packet` — no `when good packet` block) materializes as
    //     the root base's members under the spelling's name, with the
    //     spelling's words resolved as the view's determinants.
    // (b) the `typeof()`/`typeof_item()` placeholders (5.9) resolve
    //     against the composed types and rewrite in place.
    finalize_type_positions(&mut types);
    let defines = collect_defines(declarations);
    // Recover sequence declarations from the parser's synthesized
    // `<name>.item` / `<name>.created_driver` / `<name>.created_kind`
    // string defines on the sequence struct.
    let mut sequences = Vec::new();
    for declaration in declarations {
        let prefix = format!("{}.", declaration.name);
        let mut item = None;
        let mut driver = None;
        let mut kind = None;
        for member in &declaration.members {
            let Member::Define { name, value, .. } = member else {
                continue;
            };
            let Some(field) = name.strip_prefix(&prefix) else {
                continue;
            };
            let Some(crate::ast::Expr::StringLiteral { value, .. }) = value else {
                continue;
            };
            match field {
                "item" => item = Some(value.clone()),
                "created_driver" => driver = Some(value.clone()),
                "created_kind" => kind = Some(value.clone()),
                _ => {}
            }
        }
        if let (Some(item), Some(driver)) = (item, driver) {
            sequences.push(SequenceInfo {
                name: declaration.name.clone(),
                item,
                driver,
                kind: kind.unwrap_or_else(|| format!("{}_kind_t", declaration.name)),
            });
        }
    }
    // The clause 31 predefined numeric names (T7.2): the two template
    // interfaces and the `numeric_format_kind` enumerand type,
    // synthesized as elaborated types. Appended after the declared and
    // monomorphized types, so they join no when-synthesis or
    // monomorphization worklist; a file-declared name of the same kind
    // shadows the builtin (the synthesized entry is skipped).
    compose_builtin_numeric_interfaces(&mut types);
    let mut result = Elaborated {
        types,
        defines,
        sequences,
        diagnostics: elaborator.diagnostics,
        annotation_types,
        attached_annotations,
        static_fields: static_registry(declarations),
        static_methods: static_registry(declarations),
        static_events: static_registry(declarations),
        static_ons: static_registry(declarations),
    };
    result.diagnostics.extend(crate::coverage::validate_types(
        &result.types,
        &result.defines,
    ));
    result
}

/// The clause 31 predefined numeric names (T7.2), synthesized as
/// elaborated types:
///
/// - `base_numeric_i` — the conversion and radix-query interface every
///   custom numeric type realizes (clause 31.3's signatures).
/// - `numeric_i of (<me'type>, <signed'exp>: bool, <bits'exp>: int)`
///   like `base_numeric_i` — the operator interface a
///   `numeric_type`'s implementing struct realizes (clause 31.4). The
///   synthesized type is the declaration view under the template's own
///   name; the per-struct instance an `implementing numeric_i of
///   (...)` clause names carries its substituted actuals in the
///   composed implements set, and the interpreter's dispatch reads the
///   implementing struct's own methods (the names, not the signatures,
///   are what the dispatch needs).
/// - `numeric_format_kind` — the radix enumerand type of
///   `numeric_to_string` (clause 31.3).
///
/// Only names the file does not declare itself are synthesized — a
/// user declaration shadows the builtin.
fn compose_builtin_numeric_interfaces(types: &mut Vec<ElaboratedType>) {
    use crate::ast::{MethodDecl, Param, ScalarBase};
    let zero = crate::lexer::Position {
        byte: 0,
        line: 0,
        column: 0,
    };
    let builtin_span = (zero, zero);
    let named = |name: &str| TypeExpr::Named {
        name: name.to_owned(),
        span: builtin_span,
    };
    let scalar = |base: ScalarBase| TypeExpr::Scalar {
        base,
        width: None,
        span: builtin_span,
    };
    let unbounded_int = TypeExpr::Scalar {
        base: ScalarBase::Int,
        // The `int(bits: *)` marker width (5.2.3.1) the width helpers
        // and the 5.6.3 operand classification read.
        width: Some(Box::new(crate::ast::Expr::Ident {
            name: "*".to_owned(),
            span: builtin_span,
        })),
        span: builtin_span,
    };
    let list_of_bit = TypeExpr::List {
        element: Box::new(scalar(ScalarBase::Bit)),
        key: None,
        span: builtin_span,
    };
    // One signature of a synthesized interface: a body-less method with
    // the given (name, type) parameters and optional return type.
    let signature =
        |name: &str, params: &[(String, Option<TypeExpr>)], ret: Option<TypeExpr>| -> MethodDecl {
            MethodDecl {
                name: name.to_owned(),
                name_span: builtin_span,
                params: params
                    .iter()
                    .map(|(param_name, ty)| Param {
                        name: (*param_name).to_owned(),
                        name_span: builtin_span,
                        ty: ty.clone(),
                    })
                    .collect(),
                return_type: ret,
                tcm_event: None,
                modifier: None,
                is_static: false,
                annotations: Vec::new(),
                body: Vec::new(),
                span: builtin_span,
            }
        };
    let untyped = |name: &str| (name.to_owned(), None);
    let typed = |name: &str, ty: TypeExpr| (name.to_owned(), Some(ty));
    if !types.iter().any(|t| t.name == "base_numeric_i") {
        let methods = vec![
            signature("set_initial_value", &[], None),
            signature(
                "numeric_to_string",
                &[typed("radix", named("numeric_format_kind"))],
                Some(scalar(ScalarBase::String)),
            ),
            signature(
                "numeric_from_string",
                &[typed("value", scalar(ScalarBase::String))],
                None,
            ),
            signature(
                "numeric_to_longuint",
                &[],
                Some(scalar(ScalarBase::Longuint)),
            ),
            signature(
                "numeric_from_longuint",
                &[typed("value", scalar(ScalarBase::Longuint))],
                None,
            ),
            signature("numeric_to_longint", &[], Some(scalar(ScalarBase::Longint))),
            signature(
                "numeric_from_longint",
                &[typed("value", scalar(ScalarBase::Longint))],
                None,
            ),
            signature("numeric_to_unbounded", &[], Some(unbounded_int.clone())),
            signature(
                "numeric_from_unbounded",
                &[typed("value", unbounded_int.clone())],
                None,
            ),
            signature("numeric_to_real", &[], Some(scalar(ScalarBase::Real))),
            signature(
                "numeric_from_real",
                &[typed("value", scalar(ScalarBase::Real))],
                None,
            ),
            signature("numeric_to_uint", &[], Some(scalar(ScalarBase::Uint))),
            signature(
                "numeric_from_uint",
                &[typed("value", scalar(ScalarBase::Uint))],
                None,
            ),
            signature("numeric_to_int", &[], Some(scalar(ScalarBase::Int))),
            signature(
                "numeric_from_int",
                &[typed("value", scalar(ScalarBase::Int))],
                None,
            ),
            signature(
                "numeric_to_numeric",
                &[typed("other", named("base_numeric_i"))],
                None,
            ),
        ];
        types.push(ElaboratedType {
            kind: DeclarationKind::Interface,
            name: "base_numeric_i".to_owned(),
            base: None,
            fields: Vec::new(),
            constraints: Vec::new(),
            methods,
            whens: Vec::new(),
            ons: Vec::new(),
            covers: Vec::new(),
            expects: Vec::new(),
            when_of: Vec::new(),
            events: Vec::new(),
            underlying: None,
            ports: Vec::new(),
            unit_attributes: Vec::new(),
            implements: Vec::new(),
            interface_bases: Vec::new(),
            enumerals: Vec::new(),
            span: builtin_span,
        });
    }
    if !types.iter().any(|t| t.name == "numeric_i") {
        // `numeric_i`'s own operator signatures (clause 31.4), with the
        // inherited `base_numeric_i` members folded in front of them —
        // the composed form an interface elaborates to.
        let own = [
            signature("numeric_assign", &[untyped("other")], None),
            signature(
                "numeric_compare",
                &[untyped("other")],
                Some(scalar(ScalarBase::Int)),
            ),
            signature("numeric_bit_and", &[untyped("other"), untyped("res")], None),
            signature("numeric_bit_or", &[untyped("other"), untyped("res")], None),
            signature("numeric_bit_xor", &[untyped("other"), untyped("res")], None),
            signature(
                "numeric_multiply",
                &[untyped("other"), untyped("res")],
                None,
            ),
            signature("numeric_divide", &[untyped("other"), untyped("res")], None),
            signature("numeric_modulo", &[untyped("other"), untyped("res")], None),
            signature("numeric_add", &[untyped("other"), untyped("res")], None),
            signature(
                "numeric_subtract",
                &[untyped("other"), untyped("res")],
                None,
            ),
            signature(
                "numeric_shift_right",
                &[typed("n", scalar(ScalarBase::Int)), untyped("res")],
                None,
            ),
            signature(
                "numeric_shift_left",
                &[typed("n", scalar(ScalarBase::Int)), untyped("res")],
                None,
            ),
            signature("numeric_bit_not", &[untyped("res")], None),
            signature("numeric_unary_minus", &[untyped("res")], None),
            signature("numeric_unary_plus", &[untyped("res")], None),
            signature(
                "numeric_extract_bits",
                &[
                    typed("from", scalar(ScalarBase::Int)),
                    typed("to", scalar(ScalarBase::Int)),
                    typed("buffer", list_of_bit.clone()),
                ],
                None,
            ),
            signature(
                "numeric_extract_bits_assign",
                &[
                    typed("from", scalar(ScalarBase::Int)),
                    typed("to", scalar(ScalarBase::Int)),
                    typed("values", list_of_bit.clone()),
                ],
                None,
            ),
            signature("numeric_floor", &[untyped("res")], None),
            signature("numeric_ceil", &[untyped("res")], None),
            signature("numeric_round", &[untyped("res")], None),
            signature("numeric_log", &[untyped("res")], None),
            signature("numeric_log10", &[untyped("res")], None),
            signature("numeric_pow", &[untyped("exponent"), untyped("res")], None),
            signature("numeric_sqrt", &[untyped("res")], None),
            signature("numeric_exp", &[untyped("res")], None),
            signature("numeric_sin", &[untyped("res")], None),
            signature("numeric_cos", &[untyped("res")], None),
            signature("numeric_tan", &[untyped("res")], None),
            signature("numeric_asin", &[untyped("res")], None),
            signature("numeric_acos", &[untyped("res")], None),
            signature("numeric_atan", &[untyped("res")], None),
            signature("numeric_sinh", &[untyped("res")], None),
            signature("numeric_cosh", &[untyped("res")], None),
            signature("numeric_tanh", &[untyped("res")], None),
            signature("numeric_asinh", &[untyped("res")], None),
            signature("numeric_acosh", &[untyped("res")], None),
            signature("numeric_atanh", &[untyped("res")], None),
            signature("numeric_atan2", &[untyped("other"), untyped("res")], None),
            signature("numeric_hypot", &[untyped("other"), untyped("res")], None),
            signature("numeric_is_nan", &[], Some(scalar(ScalarBase::Bool))),
            signature("numeric_is_finite", &[], Some(scalar(ScalarBase::Bool))),
            signature("numeric_is_pos_inf", &[], Some(scalar(ScalarBase::Bool))),
            signature("numeric_is_neg_inf", &[], Some(scalar(ScalarBase::Bool))),
            signature("numeric_abs", &[untyped("res")], None),
            signature("numeric_odd", &[], Some(scalar(ScalarBase::Bool))),
            signature("numeric_even", &[], Some(scalar(ScalarBase::Bool))),
            signature("numeric_ilog2", &[untyped("res")], None),
            signature("numeric_ilog10", &[untyped("res")], None),
            signature("numeric_ipow", &[untyped("other"), untyped("res")], None),
            signature("numeric_isqrt", &[untyped("res")], None),
            signature(
                "numeric_div_round_up",
                &[untyped("other"), untyped("res")],
                None,
            ),
        ];
        let mut methods: Vec<MethodDecl> = types
            .iter()
            .find(|t| t.name == "base_numeric_i")
            .map(|base| base.methods.clone())
            .unwrap_or_default();
        methods.extend(own);
        types.push(ElaboratedType {
            kind: DeclarationKind::Interface,
            name: "numeric_i".to_owned(),
            base: None,
            fields: Vec::new(),
            constraints: Vec::new(),
            methods,
            whens: Vec::new(),
            ons: Vec::new(),
            covers: Vec::new(),
            expects: Vec::new(),
            when_of: Vec::new(),
            events: Vec::new(),
            underlying: None,
            ports: Vec::new(),
            unit_attributes: Vec::new(),
            implements: Vec::new(),
            interface_bases: vec!["base_numeric_i".to_owned()],
            enumerals: Vec::new(),
            span: builtin_span,
        });
    }
    if !types
        .iter()
        .any(|t| t.name == "numeric_format_kind" && t.kind == DeclarationKind::Type)
    {
        types.push(ElaboratedType {
            kind: DeclarationKind::Type,
            name: "numeric_format_kind".to_owned(),
            base: None,
            fields: Vec::new(),
            constraints: Vec::new(),
            methods: Vec::new(),
            whens: Vec::new(),
            ons: Vec::new(),
            covers: Vec::new(),
            expects: Vec::new(),
            when_of: Vec::new(),
            events: Vec::new(),
            underlying: None,
            ports: Vec::new(),
            unit_attributes: Vec::new(),
            implements: Vec::new(),
            interface_bases: Vec::new(),
            enumerals: crate::builtins::NUMERIC_FORMAT_KIND
                .iter()
                .map(|name| crate::ast::Enumeral {
                    name: (*name).to_owned(),
                    name_span: builtin_span,
                    value: None,
                })
                .collect(),
            span: builtin_span,
        });
    }
}

/// The static-member registries (see [`Elaborated`]'s fields): one
/// shared collection walk keyed by the declaring type's canonical name.
/// Members are collected in declaration order — base declaration first,
/// then each extend block in source order — which is the order the
/// interpreter composes static `on` blocks and static method bodies in.
fn static_registry<K: RegistryKind>(declarations: &[Declaration]) -> BTreeMap<String, Vec<K>> {
    let mut registry: BTreeMap<String, Vec<K>> = BTreeMap::new();
    for declaration in declarations {
        // Only struct/unit declarations and the extend blocks that
        // target them contribute; a `type` declaration has no members of
        // these kinds, and a template extension applies per instance
        // (8.2/8.3-NOTE-3), never to the type's static store.
        if !matches!(
            declaration.kind,
            DeclarationKind::Struct | DeclarationKind::Unit | DeclarationKind::Extend
        ) || declaration.template_extend.is_some()
        {
            continue;
        }
        for member in &declaration.members {
            if let Some(entry) = K::from_member(member) {
                registry
                    .entry(declaration.name.clone())
                    .or_default()
                    .push(entry);
            }
        }
    }
    registry
}

/// One static-registry value kind extracted from a member.
trait RegistryKind: Sized {
    fn from_member(member: &Member) -> Option<Self>;
}

impl RegistryKind for Field {
    fn from_member(member: &Member) -> Option<Self> {
        match member {
            Member::Field(field) if field.is_static => Some(field.clone()),
            _ => None,
        }
    }
}

impl RegistryKind for MethodDecl {
    fn from_member(member: &Member) -> Option<Self> {
        match member {
            Member::Method(method) if method.is_static => Some(method.clone()),
            _ => None,
        }
    }
}

impl RegistryKind for crate::ast::EventDecl {
    fn from_member(member: &Member) -> Option<Self> {
        match member {
            Member::Event(event) if event.is_static => Some(event.clone()),
            _ => None,
        }
    }
}

impl RegistryKind for crate::ast::OnDecl {
    fn from_member(member: &Member) -> Option<Self> {
        match member {
            // Both static forms of clause 11.2 are static-context blocks:
            // `static on <event>` and the type-qualified `on Type::event`.
            Member::On(on) if on.is_static || on.type_qualified.is_some() => Some(on.clone()),
            _ => None,
        }
    }
}

/// Fold one interface's transitive method signatures (clause 6.13): the
/// `like` bases' members first (recursively, in base order), then the
/// interface's own; a repeated method name keeps its first entry. A
/// `like` cycle is reported once per fold entry (E0301) and stops the
/// walk.
fn fold_interface_methods(
    name: &str,
    declarations: &[Declaration],
    stack: &mut Vec<String>,
    seen: &mut BTreeSet<String>,
    methods: &mut Vec<MethodDecl>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(declaration) = declarations
        .iter()
        .find(|d| d.kind == DeclarationKind::Interface && d.name == name)
    else {
        return;
    };
    if stack.iter().any(|entry| entry == name) {
        let (start, end) = declaration.name_span;
        diagnostics.push(Diagnostic::new(
            "E0301",
            format!("inheritance cycle involving '{name}'"),
            start,
            end,
        ));
        return;
    }
    stack.push(name.to_owned());
    for (base, _) in &declaration.bases {
        // The declaration view folds by the base's name; a
        // parameterised base's instance composes in the instance view.
        let Some(base_name) = base.declared_name() else {
            continue;
        };
        fold_interface_methods(base_name, declarations, stack, seen, methods, diagnostics);
    }
    for member in &declaration.members {
        let Member::Method(method) = member else {
            continue;
        };
        if seen.insert(method.name.clone()) {
            methods.push(method.clone());
        }
    }
    stack.pop();
}

impl<'a> Elaborator<'a> {
    /// A built-in unit (`sys`) is extended without being declared: fold its
    /// extend blocks into a synthesized type appended after the declared
    /// types.
    fn compose_builtin_units(
        &mut self,
        types: &mut Vec<ElaboratedType>,
        defines: &BTreeMap<String, Expr>,
        queue: &mut Vec<(String, Vec<crate::ast::TypeExpr>)>,
    ) {
        for &builtin in crate::builtins::BUILTIN_UNITS {
            if self.base_by_name.contains_key(builtin) {
                continue;
            }
            let Some(extend_indices) = self.extends_by_target.get(builtin).cloned() else {
                continue;
            };
            let mut composed = Composed::default();
            let mut port_paths: BTreeMap<String, String> = BTreeMap::new();
            for extend_index in &extend_indices {
                let extend = &self.declarations[*extend_index];
                Self::push_members(extend, *extend_index, &mut composed);
                port_paths.extend(self.extract_port_paths(extend));
            }
            // `sys` is a unit: the injected any_unit hooks (24.9.2.14)
            // apply to it as well, base-first.
            let any_unit = self.compose_any_unit();
            composed.prepend(any_unit);
            self.check_duplicate_fields(builtin, &composed);
            self.check_method_redefinitions(builtin, &composed);
            let implements =
                render_implements(&composed.implements, self.declarations, defines, queue);
            types.push(ElaboratedType {
                kind: DeclarationKind::Unit,
                name: builtin.to_owned(),
                base: None,
                fields: composed
                    .fields
                    .into_iter()
                    .map(|(field, _)| field.clone())
                    .collect(),
                constraints: composed.constraints.into_iter().cloned().collect(),
                methods: composed
                    .methods
                    .into_iter()
                    .map(|(method, _)| method.clone())
                    .collect(),
                whens: composed.whens.into_iter().cloned().collect(),
                ons: composed.ons.into_iter().map(|(on, _)| on.clone()).collect(),
                covers: composed.covers.into_iter().cloned().collect(),
                expects: composed.expects.into_iter().cloned().collect(),
                when_of: Vec::new(),
                events: composed
                    .events
                    .into_iter()
                    .map(|(event, _)| event.clone())
                    .collect(),
                underlying: None,
                enumerals: Vec::new(),
                ports: composed
                    .ports
                    .into_iter()
                    .map(|(port, _)| PortInfo {
                        name: port.name.clone(),
                        hdl_path: port_paths.get(port.name.as_str()).cloned(),
                        is_event: matches!(
                            &port.element,
                            crate::ast::TypeExpr::Named { name, .. } if name == "event"
                        ),
                    })
                    .collect(),
                unit_attributes: extend_indices
                    .iter()
                    .flat_map(|index| self.extract_unit_attributes(&self.declarations[*index]))
                    .collect(),
                implements,
                interface_bases: Vec::new(),
                span: self.declarations[extend_indices[0]].span,
            });
        }
    }

    /// `extend any_unit { ... }` members (the 24.9.2.14 message hooks):
    /// composed once and injected into every unit type by [`elaborate`].
    fn compose_any_unit(&self) -> Composed<'a> {
        let mut composed = Composed::default();
        if let Some(extend_indices) = self.extends_by_target.get("any_unit") {
            for extend_index in extend_indices {
                let extend = &self.declarations[*extend_index];
                Self::push_members(extend, *extend_index, &mut composed);
            }
        }
        composed
    }

    /// Resolve `keep <port>.hdl_path() == "path"` constraints of one
    /// declaration into a port-name → HDL-location map.
    fn extract_port_paths(&self, declaration: &'a Declaration) -> BTreeMap<String, String> {
        use crate::ast::{BinaryOp, Expr};
        let port_names: Vec<&str> = declaration
            .members
            .iter()
            .filter_map(|member| match member {
                Member::Port(port) => Some(port.name.as_str()),
                _ => None,
            })
            .collect();
        if port_names.is_empty() {
            return BTreeMap::new();
        }
        let mut paths = BTreeMap::new();
        for member in &declaration.members {
            let Member::Constraint(constraint) = member else {
                continue;
            };
            let Expr::Binary {
                op: BinaryOp::Eq,
                lhs,
                rhs,
                ..
            } = &constraint.expr
            else {
                continue;
            };
            // Either orientation of `<port>.hdl_path() == "<path>"`.
            for (call, literal) in [(lhs.as_ref(), rhs.as_ref()), (rhs.as_ref(), lhs.as_ref())] {
                let Expr::Call { callee, args, .. } = call else {
                    continue;
                };
                if !args.is_empty() {
                    continue;
                }
                let Expr::MemberAccess {
                    object,
                    name: method,
                    ..
                } = callee.as_ref()
                else {
                    continue;
                };
                let Expr::Ident { name: port, .. } = object.as_ref() else {
                    continue;
                };
                if method != "hdl_path" || !port_names.contains(&port.as_str()) {
                    continue;
                }
                if let Expr::StringLiteral { value, .. } = literal {
                    paths.insert(port.to_string(), value.clone());
                }
            }
        }
        paths
    }

    /// Record unit attribute constraints (7.3.4 `pack_options()`, 7.3.5
    /// `per_inst_cover()`) of one declaration: (field, attribute, value
    /// as written). Both orientations of
    /// `keep <field>.<attribute>() == <value>` are read, the value
    /// rendered verbatim (`packing.low_big_endian`, `DEFAULT`); a later
    /// declaration of the same field's attribute wins, like a re-set
    /// hdl path.
    fn extract_unit_attributes(
        &self,
        declaration: &'a Declaration,
    ) -> Vec<(String, String, String)> {
        use crate::ast::{BinaryOp, Expr};
        const ATTRIBUTES: &[&str] = &["pack_options", "per_inst_cover"];
        let field_names: Vec<&str> = declaration
            .members
            .iter()
            .filter_map(|member| match member {
                Member::Field(field) => Some(field.name.as_str()),
                _ => None,
            })
            .collect();
        if field_names.is_empty() {
            return Vec::new();
        }
        let mut attributes: Vec<(String, String, String)> = Vec::new();
        for member in &declaration.members {
            let Member::Constraint(constraint) = member else {
                continue;
            };
            let Expr::Binary {
                op: BinaryOp::Eq,
                lhs,
                rhs,
                ..
            } = &constraint.expr
            else {
                continue;
            };
            for (call, literal) in [(lhs.as_ref(), rhs.as_ref()), (rhs.as_ref(), lhs.as_ref())] {
                let Expr::Call { callee, args, .. } = call else {
                    continue;
                };
                if !args.is_empty() {
                    continue;
                }
                let Expr::MemberAccess {
                    object,
                    name: attribute,
                    ..
                } = callee.as_ref()
                else {
                    continue;
                };
                let Expr::Ident { name: field, .. } = object.as_ref() else {
                    continue;
                };
                if !ATTRIBUTES.contains(&attribute.as_str())
                    || !field_names.contains(&field.as_str())
                {
                    continue;
                }
                let value = crate::ast::render_expr_compact(literal);
                attributes.retain(|(recorded_field, recorded_attribute, _)| {
                    !(recorded_field == field && recorded_attribute == attribute)
                });
                attributes.push((field.clone(), attribute.clone(), value));
            }
        }
        attributes
    }

    fn compose(&mut self, name: &'a str, stack: &mut Vec<&'a str>) -> Composed<'a> {
        let Some(&index) = self.base_by_name.get(name) else {
            return Composed::default();
        };
        let declaration: &'a Declaration = &self.declarations[index];
        if declaration.kind == DeclarationKind::Type {
            // A `like` target that is an enumeral type contributes nothing;
            // full type checking is a later milestone.
            return Composed::default();
        }
        if stack.contains(&name) {
            if self.reported_cycles.insert(name) {
                let (start, end) = declaration.like_span.unwrap_or(declaration.name_span);
                self.diagnostics.push(Diagnostic::new(
                    "E0301",
                    format!("inheritance cycle involving '{name}'"),
                    start,
                    end,
                ));
            }
            return Composed::default();
        }
        stack.push(name);
        let mut composed = match &declaration.like {
            Some(base) => self.compose(base.as_str(), stack),
            None => Composed::default(),
        };
        Self::push_members(declaration, index, &mut composed);
        let extend_indices = self
            .extends_by_target
            .get(name)
            .cloned()
            .unwrap_or_default();
        for extend_index in extend_indices {
            let extend = &self.declarations[extend_index];
            Self::push_members(extend, extend_index, &mut composed);
        }
        self.check_duplicate_fields(name, &composed);
        self.check_method_redefinitions(name, &composed);
        stack.pop();
        composed
    }

    fn push_members(declaration: &'a Declaration, block: usize, composed: &mut Composed<'a>) {
        // The `implementing` clause rides with the contributing block
        // (clause 6.3): a like-chain base's interfaces, the declaration's
        // own, and each extend block's compose in written order, deduped
        // by the entries' rendered spelling.
        for interface in &declaration.implements {
            if !composed
                .implements
                .iter()
                .any(|recorded| recorded.render() == interface.render())
            {
                composed.implements.push(interface.clone());
            }
        }
        for member in &declaration.members {
            match member {
                Member::Field(field) => composed.fields.push((field, block)),
                Member::Constraint(constraint) => composed.constraints.push(constraint),
                Member::Method(method) => composed.methods.push((method, block)),
                Member::When(when) => composed.whens.push(when),
                Member::On(on) => composed.ons.push((on, block)),
                Member::Cover(cover) => composed.covers.push(cover),
                Member::Expect(expect) => composed.expects.push(expect),
                Member::Port(port) => composed.ports.push((port, block)),
                Member::Event(event) => composed.events.push((event, block)),
                Member::Import { .. } | Member::Define { .. } | Member::Macro(_) => {}
                Member::Enumeral(_) => {}
            }
        }
    }

    /// One diagnostic per field name that two different blocks contribute;
    /// duplicates inside one block are the parser's `E0202`.
    fn check_duplicate_fields(&mut self, type_name: &'a str, composed: &Composed<'a>) {
        for i in 0..composed.fields.len() {
            for j in i + 1..composed.fields.len() {
                let (field_i, block_i) = &composed.fields[i];
                let (field_j, block_j) = &composed.fields[j];
                if field_i.name == field_j.name
                    && block_i != block_j
                    && self.reported_fields.insert(field_j.name.as_str())
                {
                    self.diagnostics.push(Diagnostic::new(
                        "E0302",
                        format!(
                            "duplicate field '{}' in elaborated type '{}'",
                            field_j.name, type_name
                        ),
                        field_j.name_span.0,
                        field_j.name_span.1,
                    ));
                }
            }
        }
    }

    /// One diagnostic per method name that is redefined without an
    /// extension modifier.
    fn check_method_redefinitions(&mut self, type_name: &'a str, composed: &Composed<'a>) {
        for i in 0..composed.methods.len() {
            let (method, _) = &composed.methods[i];
            if method.modifier.is_some() {
                continue;
            }
            let redefined = composed.methods[..i]
                .iter()
                .any(|(earlier, _)| earlier.name == method.name);
            if redefined && self.reported_methods.insert(method.name.as_str()) {
                self.diagnostics.push(Diagnostic::new(
                    "E0303",
                    format!(
                        "method '{}' is redefined in elaborated type '{}' without 'is also', 'is first' or 'is only'",
                        method.name, type_name
                    ),
                    method.name_span.0,
                    method.name_span.1,
                ));
            }
        }
    }
}

/// The unmarked base type a synthesized when view (or chain of views)
/// was built from: the enclosing view's own name for a base type, the
/// innermost `base` otherwise — every when view's base chain ends at a
/// type whose [`ElaboratedType::when_of`] is empty.
fn when_root_base_name(types: &[ElaboratedType], index: usize) -> String {
    let mut cursor = index;
    while !types[cursor].when_of.is_empty() {
        let base = types[cursor]
            .base
            .as_deref()
            .expect("a synthesized when view always carries a base");
        cursor = types
            .iter()
            .position(|ty| ty.name == base)
            .expect("the view's base type is elaborated");
    }
    types[cursor].name.clone()
}

/// A when block's declared subtype: the (field id, value) determinant
/// pairs, and the value words the synthesized view's name is built from
/// (`red good` for `when red good packet` — the written words, not the
/// resolved determinant values).
struct WhenSubtype {
    pairs: Vec<(String, String)>,
    name_words: Vec<String>,
}

/// The subtype a when block's modifiers declare, or `None` when the
/// modifier spelling does not identify its determinant fields. The
/// spellings (6.7.1 — subtypes can be combinations of fields):
///
/// - the conventional `value'id` modifiers, any number of them (`when
///   BUSY'kind`, `when LO'kind HI'dir`);
/// - a trailing bare modifier equal to the enclosing (or root base)
///   type's name is that type's name marker, not a subtype value (`when
///   red good packet` on `packet`) — it is stripped first;
/// - bare modifier words, each a subtype value: the Boolean field it
///   names (`good` for `TRUE'good`) or the field whose enumeral type
///   contains it (`red` of `color`) — `when red good packet` fixes two
///   determinants and names the view `red good packet`;
/// - the corpus's space form `when NO_CONSTRAINT env` (two bare
///   modifiers — value first, field id second), and the `when <value>
///   <struct-name>` form, as the fallback readings when the fields'
///   types do not identify the words. There the second word is a
///   determinant id, not a value, so it stays out of the view's name.
fn when_modifier_pairs(
    when: &WhenDecl,
    view: &ElaboratedType,
    root_base: &str,
    types: &[ElaboratedType],
) -> Option<WhenSubtype> {
    if when.modifiers.is_empty() {
        return None;
    }
    let mut modifiers = when.modifiers.as_slice();
    let mut marker: Option<&str> = None;
    if let Some(last) = modifiers.last()
        && last.id.is_none()
        && (last.value == view.name || last.value == root_base)
    {
        marker = Some(last.value.as_str());
        modifiers = &modifiers[..modifiers.len() - 1];
    }
    if modifiers.is_empty() {
        return None;
    }
    // The conventional form: every modifier names its determinant.
    if modifiers.iter().all(|modifier| modifier.id.is_some()) {
        return Some(WhenSubtype {
            pairs: modifiers
                .iter()
                .map(|modifier| {
                    let (id, _) = modifier.id.as_ref().expect("checked above");
                    (id.clone(), modifier.value.clone())
                })
                .collect(),
            name_words: modifiers
                .iter()
                .map(|modifier| modifier.value.clone())
                .collect(),
        });
    }
    // All-bare modifiers: subtype value words.
    if modifiers.iter().all(|modifier| modifier.id.is_none()) {
        let words: Vec<&str> = modifiers.iter().map(|m| m.value.as_str()).collect();
        if let Some(pairs) = resolve_bare_words(&words, view, types) {
            return Some(WhenSubtype {
                pairs,
                name_words: words.iter().map(|w| w.to_string()).collect(),
            });
        }
        // The fallback readings preserve the pre-existing behavior:
        // `when <value> <struct-name>` (the name marker is the pair's
        // id), and the corpus's space form (value first, field id
        // second) — the second word is an id there, not a value.
        if let (Some(marker), [word]) = (marker, words.as_slice()) {
            return Some(WhenSubtype {
                pairs: vec![(marker.to_string(), word.to_string())],
                name_words: vec![word.to_string()],
            });
        }
        if let [first, second] = words.as_slice() {
            return Some(WhenSubtype {
                pairs: vec![(second.to_string(), first.to_string())],
                name_words: vec![first.to_string()],
            });
        }
        return None;
    }
    // Mixed `value'id` and bare modifiers: the bare words resolve like
    // the all-bare form; one unresolvable word means the spelling does
    // not identify its determinants.
    let mut pairs = Vec::new();
    let mut name_words = Vec::new();
    for modifier in modifiers {
        if let Some((id, _)) = &modifier.id {
            pairs.push((id.clone(), modifier.value.clone()));
        } else {
            let word_pairs = resolve_bare_words(&[modifier.value.as_str()], view, types)?;
            pairs.extend(word_pairs);
        }
        name_words.push(modifier.value.clone());
    }
    Some(WhenSubtype { pairs, name_words })
}

/// The (field id, value) pairs bare subtype words declare: the Boolean
/// field a word names (`good` for `TRUE'good`) or the field whose
/// enumeral type contains the word (`red` of `color`). One
/// unidentifiable word fails the whole sequence.
fn resolve_bare_words(
    words: &[&str],
    view: &ElaboratedType,
    types: &[ElaboratedType],
) -> Option<Vec<(String, String)>> {
    let mut pairs = Vec::new();
    for word in words {
        let word = *word;
        // A Boolean field named by the word (the bare `TRUE'good` form).
        if let Some(field) = view.fields.iter().find(|field| field.name == word)
            && matches!(
                &field.ty,
                crate::ast::TypeExpr::Scalar {
                    base: crate::ast::ScalarBase::Bool,
                    ..
                }
            )
        {
            pairs.push((word.to_string(), "TRUE".to_string()));
            continue;
        }
        // An enumerand of a field's declared or inline enumeral type.
        let mut determinant = None;
        for field in &view.fields {
            let holds = match &field.ty {
                crate::ast::TypeExpr::Named { name, .. } => types.iter().any(|ty| {
                    &ty.name == name
                        && ty.kind == DeclarationKind::Type
                        && ty.enumerals.iter().any(|enumeral| enumeral.name == word)
                }),
                crate::ast::TypeExpr::InlineEnum { names, .. } => names.iter().any(|n| n == word),
                _ => false,
            };
            if holds {
                determinant = Some(field.name.clone());
                break;
            }
        }
        let field_name = determinant?;
        pairs.push((field_name, word.to_string()));
    }
    Some(pairs)
}

/// The substitution context of one template instantiation (8.3): the
/// type formals' actuals, the value formals' constants, the file's
/// `define` constants, the declarations (template lookups for 8.5.4's
/// default filling) and the monomorphization worklist.
struct Subst<'d> {
    declarations: &'d [Declaration],
    types: BTreeMap<String, crate::ast::TypeExpr>,
    values: BTreeMap<String, Expr>,
    defines: &'d BTreeMap<String, Expr>,
    queue: &'d mut Vec<(String, Vec<crate::ast::TypeExpr>)>,
}

impl<'d> Subst<'d> {
    /// An empty binding: the formals stay unresolved, while template
    /// references still fill their defaults and seed the queue — the
    /// seed pass over the composed types and the expression walks that
    /// carry no type bindings run through it.
    fn new(
        declarations: &'d [Declaration],
        defines: &'d BTreeMap<String, Expr>,
        queue: &'d mut Vec<(String, Vec<crate::ast::TypeExpr>)>,
    ) -> Self {
        Subst {
            declarations,
            types: BTreeMap::new(),
            values: BTreeMap::new(),
            defines,
            queue,
        }
    }
}

/// Substitute the bound actuals for the formals of a type position
/// (8.3), enqueueing every template reference the substitution
/// uncovers so the worklist materializes their instances. The type
/// formals (`<t'type>`) read the context's type bindings; the value
/// formals (`<n'exp>`) — including inside scalar widths — read the
/// value bindings; a template reference's trailing defaults fill from
/// the referenced template's own (8.5.4).
fn substitute_type(ty: &crate::ast::TypeExpr, ctx: &mut Subst) -> crate::ast::TypeExpr {
    match ty {
        crate::ast::TypeExpr::Formal { name, span } => {
            ctx.types
                .get(name)
                .cloned()
                .unwrap_or(crate::ast::TypeExpr::Formal {
                    name: name.clone(),
                    span: *span,
                })
        }
        // A scalar width is an expression position: a value parameter
        // over it substitutes to the constant (8.5.2 — field widths).
        crate::ast::TypeExpr::Scalar { base, width, span } => {
            let substituted_width = width.as_ref().map(|width| {
                let mut width = (**width).clone();
                substitute_expr(&mut width, ctx);
                Box::new(width)
            });
            crate::ast::TypeExpr::Scalar {
                base: *base,
                width: substituted_width,
                span: *span,
            }
        }
        crate::ast::TypeExpr::FormalWidth { name, width, span } => {
            // The substituted actual takes the substituted width when it
            // is a scalar; other actuals keep the qualified spelling
            // over the actual's rendering.
            let substituted_width = {
                let mut expr = (**width).clone();
                substitute_expr(&mut expr, ctx);
                expr
            };
            let resolved =
                ctx.types
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| crate::ast::TypeExpr::Formal {
                        name: name.clone(),
                        span: *span,
                    });
            match resolved {
                crate::ast::TypeExpr::Scalar { base, span, .. } => crate::ast::TypeExpr::Scalar {
                    base,
                    width: Some(Box::new(substituted_width)),
                    span,
                },
                resolved => crate::ast::TypeExpr::FormalWidth {
                    name: resolved.render(),
                    width: Box::new(substituted_width),
                    span: *span,
                },
            }
        }
        crate::ast::TypeExpr::List { element, key, span } => crate::ast::TypeExpr::List {
            element: Box::new(substitute_type(element, ctx)),
            key: key.as_ref().map(|key| {
                let mut key = (**key).clone();
                substitute_expr(&mut key, ctx);
                Box::new(key)
            }),
            span: *span,
        },
        crate::ast::TypeExpr::WhenQualified { value, base, span } => {
            crate::ast::TypeExpr::WhenQualified {
                value: value.clone(),
                base: Box::new(substitute_type(base, ctx)),
                span: *span,
            }
        }
        // A `typeof()` placeholder's argument is an expression whose
        // type positions substitute too (8.3 covers the whole body);
        // the placeholder itself resolves after instantiation.
        crate::ast::TypeExpr::TypeOf { expr, span } => {
            let mut expr = (**expr).clone();
            substitute_expr(&mut expr, ctx);
            crate::ast::TypeExpr::TypeOf {
                expr: Box::new(expr),
                span: *span,
            }
        }
        crate::ast::TypeExpr::TypeOfItem { expr, span } => {
            let mut expr = (**expr).clone();
            substitute_expr(&mut expr, ctx);
            crate::ast::TypeExpr::TypeOfItem {
                expr: Box::new(expr),
                span: *span,
            }
        }
        crate::ast::TypeExpr::TemplateRef { name, args, span } => {
            let substituted: Vec<crate::ast::TypeExpr> =
                args.iter().map(|arg| substitute_type(arg, ctx)).collect();
            // 8.5.4 with 8.3: the referenced template's trailing
            // defaults fill the omitted arguments, so both the
            // canonical name and the interpreter's instance lookup see
            // the same full actual list.
            let filled = fill_template_defaults(name, substituted, ctx);
            ctx.queue.push((name.clone(), filled.clone()));
            crate::ast::TypeExpr::TemplateRef {
                name: name.clone(),
                args: filled,
                span: *span,
            }
        }
        other => other.clone(),
    }
}

/// Fill a template reference's omitted trailing arguments from the
/// template declaration's defaults (8.5.4), normalizing every value
/// actual to its evaluated constant. An unknown template name or a
/// parameter with neither actual nor default leaves the reference as
/// written.
fn fill_template_defaults(
    template_name: &str,
    args: Vec<crate::ast::TypeExpr>,
    ctx: &mut Subst,
) -> Vec<crate::ast::TypeExpr> {
    let Some(template) = ctx.declarations.iter().find(|declaration| {
        declaration.name == template_name
            && !declaration.template_params.is_empty()
            && declaration.template_extend.is_none()
    }) else {
        return args;
    };
    match bind_template_actuals(ctx.declarations, template, &args, ctx.defines, ctx.queue) {
        Some(binding) => binding.actuals,
        None => args,
    }
}

/// Substitute one method's signatures and body (8.3: the substitution
/// covers the whole template body, bodies included): parameter types,
/// the return type, and every type position inside the actions. Each
/// template reference the substitution uncovers joins the worklist.
fn substitute_method(mut method: MethodDecl, ctx: &mut Subst) -> MethodDecl {
    for param in &mut method.params {
        if let Some(ty) = &param.ty {
            param.ty = Some(substitute_type(ty, ctx));
        }
    }
    if let Some(return_type) = &method.return_type {
        method.return_type = Some(substitute_type(return_type, ctx));
    }
    substitute_statements(&mut method.body, ctx);
    method
}

/// Substitute every type position inside a statement list, in place.
fn substitute_statements(statements: &mut [Stmt], ctx: &mut Subst) {
    for statement in statements {
        substitute_stmt(statement, ctx);
    }
}

/// The type positions of one statement (8.3): local-variable types,
/// `new <type>` creations, casts, `is a` checks — recursively through
/// the statement tree.
fn substitute_stmt(statement: &mut Stmt, ctx: &mut Subst) {
    match statement {
        Stmt::VarDecl { ty, init, .. } => {
            if let Some(ty) = ty {
                *ty = substitute_type(ty, ctx);
            }
            if let Some(init) = init {
                substitute_expr(init, ctx);
            }
        }
        Stmt::Assign { target, value, .. } => {
            substitute_expr(target, ctx);
            substitute_expr(value, ctx);
        }
        Stmt::ExprStmt { expr, .. } => substitute_expr(expr, ctx),
        Stmt::Return {
            value: Some(value), ..
        } => substitute_expr(value, ctx),
        Stmt::If {
            cond,
            then_body,
            else_body,
            ..
        } => {
            substitute_expr(cond, ctx);
            substitute_statements(then_body, ctx);
            if let Some(else_body) = else_body {
                substitute_statements(else_body, ctx);
            }
        }
        Stmt::While { cond, body, .. } => {
            substitute_expr(cond, ctx);
            substitute_statements(body, ctx);
        }
        Stmt::RepeatUntil { body, cond, .. } => {
            substitute_statements(body, ctx);
            substitute_expr(cond, ctx);
        }
        Stmt::Print { exprs, .. } => {
            for expr in exprs {
                substitute_expr(expr, ctx);
            }
        }
        Stmt::Gen { expr, .. } => substitute_expr(expr, ctx),
        Stmt::Check {
            condition,
            else_body,
            ..
        } => {
            substitute_expr(condition, ctx);
            substitute_statements(else_body, ctx);
        }
        Stmt::ForEach {
            using, list, body, ..
        } => {
            if let Some(using) = using {
                substitute_expr(using, ctx);
            }
            substitute_expr(list, ctx);
            substitute_statements(body, ctx);
        }
        Stmt::ForTo { from, to, body, .. } => {
            substitute_expr(from, ctx);
            substitute_expr(to, ctx);
            substitute_statements(body, ctx);
        }
        Stmt::Wait { cycles, .. } => substitute_expr(cycles, ctx),
        Stmt::Start { call, .. } | Stmt::Compute { call, .. } => substitute_expr(call, ctx),
        Stmt::Case {
            scrutinee, arms, ..
        } => {
            substitute_expr(scrutinee, ctx);
            for (alternatives, body) in arms {
                for expr in alternatives {
                    substitute_expr(expr, ctx);
                }
                substitute_statements(body, ctx);
            }
        }
        Stmt::Emit { target, .. } => substitute_expr(target, ctx),
        // Statements without expression or type positions (`break`,
        // `continue`, `stop_run`, sequence `do`) and the temporal
        // forms (a `TemporalExpr` carries no type positions) need no
        // substitution.
        _ => {}
    }
}

/// The type and value positions of one expression (8.3): `new <type>`,
/// `expr.as_a(<type>)`, `expr is [not] a <type>`,
/// `set_of_values(<type>)`, and — 8.5.2's value-parameter
/// substitution — every `<n'exp>` formal, replaced with the bound
/// constant. Recursively through the expression tree.
fn substitute_expr(expr: &mut Expr, ctx: &mut Subst) {
    match expr {
        Expr::Formal { name, .. } => {
            if let Some(actual) = ctx.values.get(name) {
                let mut substituted = actual.clone();
                substitute_expr(&mut substituted, ctx);
                *expr = substituted;
            }
        }
        Expr::New { ty, inits, .. } => {
            if let Some(ty) = ty {
                *ty = substitute_type(ty, ctx);
            }
            substitute_statements(inits, ctx);
        }
        Expr::CastAs {
            expr: inner, ty, ..
        }
        | Expr::IsA {
            expr: inner, ty, ..
        } => {
            substitute_expr(inner, ctx);
            *ty = substitute_type(ty, ctx);
        }
        Expr::SetOfValues { ty, .. } => {
            *ty = substitute_type(ty, ctx);
        }
        // A subtype-qualified access's base carries the type positions.
        Expr::QualifiedField { base, .. } => substitute_expr(base, ctx),
        Expr::MemberAccess { object, .. } => substitute_expr(object, ctx),
        Expr::Call { callee, args, .. } => {
            substitute_expr(callee, ctx);
            for arg in args {
                substitute_expr(arg, ctx);
            }
        }
        Expr::Index { base, index, .. } => {
            substitute_expr(base, ctx);
            substitute_expr(index, ctx);
        }
        Expr::Binary { lhs, rhs, .. } => {
            substitute_expr(lhs, ctx);
            substitute_expr(rhs, ctx);
        }
        Expr::Unary { expr: inner, .. } => substitute_expr(inner, ctx),
        Expr::Paren { expr: inner, .. } => substitute_expr(inner, ctx),
        Expr::Ternary {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            substitute_expr(cond, ctx);
            substitute_expr(then_expr, ctx);
            substitute_expr(else_expr, ctx);
        }
        Expr::BitSlice {
            base, high, low, ..
        } => {
            substitute_expr(base, ctx);
            substitute_expr(high, ctx);
            substitute_expr(low, ctx);
        }
        Expr::InSet { element, set, .. } => {
            substitute_expr(element, ctx);
            substitute_in_set_expr(set, ctx);
        }
        Expr::SetLiteral { items, .. } => substitute_in_set_expr(items, ctx),
        Expr::ListConcat { items, .. } => {
            for item in items {
                substitute_expr(item, ctx);
            }
        }
        Expr::Evaluate { body, .. } => substitute_statements(body, ctx),
        // Literals, identifiers, HDL paths and select distributions
        // (choices are raw tokens) carry no type positions.
        _ => {}
    }
}

/// The type positions of one `[...]` set shape (the `in` right-hand
/// side and the set literal), shared by [`substitute_expr`].
fn substitute_in_set_expr(set: &mut crate::ast::InSetExpr, ctx: &mut Subst) {
    match set {
        crate::ast::InSetExpr::Items(items) => {
            for item in items {
                substitute_expr(item, ctx);
            }
        }
        crate::ast::InSetExpr::Range(low, high) => {
            substitute_expr(low, ctx);
            substitute_expr(high, ctx);
        }
        crate::ast::InSetExpr::Expr(expr) => substitute_expr(expr, ctx),
    }
}

/// 8.6's matching rule with 8.5.3's bounding criteria: a template
/// extension applies to an instance only if every actual matches its
/// positional entry. A parameter form binds the actual to the
/// extension's own formal — a `<tag'type>` with no category imposes no
/// constraint, a category checks the actual's shape ([`actual_in_category`]),
/// a `<tag'exp>` binds the actual's constant; a concrete type-name or
/// constant constrains the match to exactly that spelling / value.
/// Returns the extension's own (type-formal, value-formal) binding
/// maps for its body's substitution.
fn match_template_extend_entries(
    entries: &[crate::ast::TemplateExtendEntry],
    actuals: &[crate::ast::TypeExpr],
    declarations: &[Declaration],
    defines: &BTreeMap<String, Expr>,
) -> Option<(
    BTreeMap<String, crate::ast::TypeExpr>,
    BTreeMap<String, Expr>,
)> {
    let mut types: BTreeMap<String, crate::ast::TypeExpr> = BTreeMap::new();
    let mut values: BTreeMap<String, Expr> = BTreeMap::new();
    for (entry, actual) in entries.iter().zip(actuals) {
        match entry {
            crate::ast::TemplateExtendEntry::TypeParam { name, category } => {
                if matches!(actual, crate::ast::TypeExpr::Value { .. }) {
                    return None;
                }
                if let Some(category) = category
                    && !actual_in_category(actual, category, declarations)
                {
                    return None;
                }
                types.insert(name.clone(), actual.clone());
            }
            crate::ast::TemplateExtendEntry::ValueParam { name, scalar } => {
                let crate::ast::TypeExpr::Value { expr, .. } = actual else {
                    return None;
                };
                let constant = eval_const_expr(expr, &values, defines, 0)?;
                if !value_constant_matches_type(&constant, scalar, declarations) {
                    return None;
                }
                values.insert(name.clone(), constant.as_expr());
                types.insert(name.clone(), actual.clone());
            }
            crate::ast::TemplateExtendEntry::Concrete(concrete) => {
                if matches!(actual, crate::ast::TypeExpr::Value { .. }) {
                    return None;
                }
                if actual.render() != concrete.render() {
                    return None;
                }
            }
            crate::ast::TemplateExtendEntry::Constant(expected) => {
                let crate::ast::TypeExpr::Value { expr, .. } = actual else {
                    return None;
                };
                let constant = eval_const_expr(expr, &values, defines, 0)?;
                let expected_constant = eval_const_expr(expected, &values, defines, 0)?;
                if constant != expected_constant {
                    return None;
                }
            }
        }
    }
    Some((types, values))
}

/// Whether a value constant's kind matches the scalar type a value
/// parameter (or extension entry) declares (8.5.2): an integer for the
/// integral bases and named scalar types, a real for `real` (an
/// integer actual promotes), a boolean for `bool`, a string for
/// `string`. An enumerand constant is not modeled — the enumerated
/// value-parameter actuals stay outside this milestone.
pub(crate) fn value_constant_matches_type(
    constant: &Const,
    scalar: &crate::ast::TypeExpr,
    declarations: &[Declaration],
) -> bool {
    match scalar {
        crate::ast::TypeExpr::Scalar { base, .. } => match base {
            crate::ast::ScalarBase::Bool => matches!(constant, Const::Bool(_)),
            crate::ast::ScalarBase::String => matches!(constant, Const::Str(_)),
            crate::ast::ScalarBase::Real => {
                matches!(constant, Const::Real(_) | Const::Int(_))
            }
            _ => matches!(constant, Const::Int(_)),
        },
        // A named scalar type (an enumerated type or a scalar subtype):
        // the constant model only carries integers here.
        crate::ast::TypeExpr::Named { .. } => matches!(constant, Const::Int(_)),
        _ => {
            let _ = declarations;
            false
        }
    }
}

/// The `implementing` entries of a composed type, rendered to the
/// canonical names the conformance checks and the interpreter read
/// (clause 6.3): a concrete entry passes through; a parameterised one
/// (`I of <t'type>`, clauses 8.2.3/8.7) renders to its instance's
/// canonical name — enqueued so the interface instance materializes.
fn render_implements(
    implements: &[TypeExpr],
    declarations: &[Declaration],
    defines: &BTreeMap<String, Expr>,
    queue: &mut Vec<(String, Vec<crate::ast::TypeExpr>)>,
) -> Vec<String> {
    let mut ctx = Subst::new(declarations, defines, queue);
    let mut rendered: Vec<String> = Vec::new();
    for interface in implements {
        let substituted = substitute_type(interface, &mut ctx);
        let name = substituted.render();
        if !rendered.contains(&name) {
            rendered.push(name);
        }
    }
    rendered
}

/// Substitute one condition (or other expression) through an
/// instance's bindings (8.5.3): the well-formedness pass reads the
/// substituted form's constant value; type positions inside it fill
/// defaults without template lookups (conditions are value
/// expressions).
pub(crate) fn substitute_condition(
    expr: &mut Expr,
    type_bindings: &BTreeMap<String, TypeExpr>,
    value_bindings: &BTreeMap<String, Expr>,
    defines: &BTreeMap<String, Expr>,
    queue: &mut Vec<(String, Vec<TypeExpr>)>,
) {
    let mut ctx = Subst {
        declarations: &[],
        types: type_bindings.clone(),
        values: value_bindings.clone(),
        defines,
        queue,
    };
    substitute_expr(expr, &mut ctx);
}

/// Fill one composed type's template references' trailing defaults
/// (8.5.4) and enqueue them for monomorphization (8.3): field types,
/// method signatures and bodies, constraints, `on` bodies. Empty
/// bindings — the formals of the raw template views stay unresolved;
/// the instances the substitution uncovers materialize through the
/// worklist.
fn substitute_entry(
    entry: &mut ElaboratedType,
    declarations: &[Declaration],
    defines: &BTreeMap<String, Expr>,
    queue: &mut Vec<(String, Vec<crate::ast::TypeExpr>)>,
) {
    let mut ctx = Subst::new(declarations, defines, queue);
    for field in &mut entry.fields {
        field.ty = substitute_type(&field.ty, &mut ctx);
    }
    for constraint in &mut entry.constraints {
        substitute_expr(&mut constraint.expr, &mut ctx);
    }
    for method in &mut entry.methods {
        *method = substitute_method(method.clone(), &mut ctx);
    }
    for on in &mut entry.ons {
        substitute_statements(&mut on.body, &mut ctx);
    }
}

/// The two final passes over the composed types (see the call site in
/// [`elaborate`]): when-qualified spelling views, then `typeof()`
/// resolution.
fn finalize_type_positions(types: &mut Vec<ElaboratedType>) {
    // (a) Spelled when-qualified views. Bounded passes: a synthesized
    // view's own field types may name further spellings.
    for _ in 0..3 {
        let mut spellings: Vec<(String, Vec<String>)> = Vec::new();
        for entry in types.iter() {
            collect_spelled_views(entry, &mut spellings);
        }
        spellings.dedup_by(|a, b| a.0 == b.0);
        if spellings.is_empty() {
            break;
        }
        let mut added = 0usize;
        for (name, words) in spellings {
            if types.iter().any(|ty| ty.name == name) {
                continue;
            }
            if synthesize_spelled_view(&name, &words, types).is_some() {
                added += 1;
            }
        }
        if added == 0 {
            break;
        }
    }
    // (b) `typeof()`/`typeof_item()` resolution (5.9), in type order so
    // a field's resolved type feeds later resolutions.
    for index in 0..types.len() {
        resolve_type_typeof(types, index);
    }
}

/// The when-qualified type spellings (6.7) one composed type's type
/// positions use, as (view name, qualifier words) candidates.
fn collect_spelled_views(entry: &ElaboratedType, out: &mut Vec<(String, Vec<String>)>) {
    let mut spellings: Vec<(String, Vec<String>)> = Vec::new();
    for field in &entry.fields {
        collect_type_spellings(&field.ty, &mut spellings);
    }
    for method in &entry.methods {
        for param in &method.params {
            if let Some(ty) = &param.ty {
                collect_type_spellings(ty, &mut spellings);
            }
        }
        if let Some(return_type) = &method.return_type {
            collect_type_spellings(return_type, &mut spellings);
        }
        for statement in &method.body {
            collect_statement_spellings(statement, &mut spellings);
        }
    }
    out.append(&mut spellings);
}

fn collect_type_spellings(ty: &crate::ast::TypeExpr, out: &mut Vec<(String, Vec<String>)>) {
    match ty {
        crate::ast::TypeExpr::WhenQualified { value, base, .. } => {
            if let Some(view_name) = crate::interp::when_view_name(value, base) {
                out.push((view_name, crate::interp::when_spelling_words(value, base)));
            }
            collect_type_spellings(base, out);
        }
        crate::ast::TypeExpr::List { element, .. } => collect_type_spellings(element, out),
        crate::ast::TypeExpr::TemplateRef { args, .. } => {
            for arg in args {
                collect_type_spellings(arg, out);
            }
        }
        _ => {}
    }
}

/// The type positions of one statement — the read-only walk that gathers
/// spelled when-qualified views from method bodies.
fn collect_statement_spellings(statement: &crate::ast::Stmt, out: &mut Vec<(String, Vec<String>)>) {
    match statement {
        crate::ast::Stmt::VarDecl { ty: Some(ty), .. } => collect_type_spellings(ty, out),
        crate::ast::Stmt::Assign { target, value, .. } => {
            collect_expression_spellings(target, out);
            collect_expression_spellings(value, out);
        }
        crate::ast::Stmt::ExprStmt { expr, .. } => collect_expression_spellings(expr, out),
        crate::ast::Stmt::Return {
            value: Some(value), ..
        } => collect_expression_spellings(value, out),
        crate::ast::Stmt::If {
            cond,
            then_body,
            else_body,
            ..
        } => {
            collect_expression_spellings(cond, out);
            for statement in then_body {
                collect_statement_spellings(statement, out);
            }
            if let Some(else_body) = else_body {
                for statement in else_body {
                    collect_statement_spellings(statement, out);
                }
            }
        }
        crate::ast::Stmt::While { cond, body, .. } => {
            collect_expression_spellings(cond, out);
            for statement in body {
                collect_statement_spellings(statement, out);
            }
        }
        crate::ast::Stmt::Case {
            scrutinee, arms, ..
        } => {
            collect_expression_spellings(scrutinee, out);
            for (alternatives, body) in arms {
                for expr in alternatives {
                    collect_expression_spellings(expr, out);
                }
                for statement in body {
                    collect_statement_spellings(statement, out);
                }
            }
        }
        _ => {}
    }
}

fn collect_expression_spellings(expr: &crate::ast::Expr, out: &mut Vec<(String, Vec<String>)>) {
    match expr {
        crate::ast::Expr::New {
            ty: Some(ty),
            inits,
            ..
        } => {
            collect_type_spellings(ty, out);
            for statement in inits {
                collect_statement_spellings(statement, out);
            }
        }
        crate::ast::Expr::CastAs { expr, ty, .. } | crate::ast::Expr::IsA { expr, ty, .. } => {
            collect_expression_spellings(expr, out);
            collect_type_spellings(ty, out);
        }
        crate::ast::Expr::SetOfValues { ty, .. } => collect_type_spellings(ty, out),
        crate::ast::Expr::MemberAccess { object, .. } => collect_expression_spellings(object, out),
        crate::ast::Expr::QualifiedField { base, .. } => collect_expression_spellings(base, out),
        crate::ast::Expr::Call { callee, args, .. } => {
            collect_expression_spellings(callee, out);
            for arg in args {
                collect_expression_spellings(arg, out);
            }
        }
        crate::ast::Expr::Index { base, index, .. } => {
            collect_expression_spellings(base, out);
            collect_expression_spellings(index, out);
        }
        crate::ast::Expr::Binary { lhs, rhs, .. } => {
            collect_expression_spellings(lhs, out);
            collect_expression_spellings(rhs, out);
        }
        crate::ast::Expr::Unary { expr, .. } | crate::ast::Expr::Paren { expr, .. } => {
            collect_expression_spellings(expr, out)
        }
        _ => {}
    }
}

/// Materialize one spelled when-qualified view: the root base's members
/// under the spelling's name, its qualifier words resolved as the view's
/// determinants. A spelling whose words do not resolve against the root
/// base's fields, or whose root base is an enumeral type, synthesizes
/// nothing — the interpreter's base-carried fallback covers the instance.
fn synthesize_spelled_view(
    view_name: &str,
    words: &[String],
    types: &mut Vec<ElaboratedType>,
) -> Option<()> {
    let root_name = view_name.rsplit(' ').next()?.to_owned();
    let root_index = types
        .iter()
        .position(|ty| ty.name == root_name && ty.kind != DeclarationKind::Type)?;
    let root = &types[root_index];
    let word_refs: Vec<&str> = words.iter().map(String::as_str).collect();
    let pairs = resolve_bare_words(&word_refs, root, types)?;
    let mut view = root.clone();
    view.name = view_name.to_owned();
    view.base = Some(root_name);
    view.when_of = pairs;
    // The spelling's own when blocks (if any exist for the exact
    // spelling) already synthesized a view under this name — this
    // materialization only fills the gap, so no when composition runs
    // on top of it.
    view.whens = Vec::new();
    types.push(view);
    Some(())
}

/// Resolve the `typeof()`/`typeof_item()` placeholders (5.9) of one
/// composed type in place: field types in declaration order (an
/// earlier field's resolved type feeds the next: `n: typeof(first_item)`
/// after `first_item: typeof_item(data)`), then each method's parameter,
/// return and local-variable types. A placeholder whose argument's
/// static type cannot be determined stays — the interpreter's value
/// model yields nothing for it, and the checker has reported `E0208`.
fn resolve_type_typeof(types: &mut [ElaboratedType], index: usize) {
    // Phase 1 (read-only over `types`): the rewritten field types.
    let mut env: BTreeMap<String, crate::ast::TypeExpr> = types[index]
        .fields
        .iter()
        .map(|field| (field.name.clone(), field.ty.clone()))
        .collect();
    let mut new_field_types = Vec::with_capacity(types[index].fields.len());
    for field in &types[index].fields {
        let resolved = resolve_typeof_in_type(&field.ty, &env, types);
        env.insert(field.name.clone(), resolved.clone());
        new_field_types.push(resolved);
    }
    // Phase 2 (write-back).
    for (position, field) in types[index].fields.iter_mut().enumerate() {
        field.ty = new_field_types[position].clone();
    }
    // Methods: parameters, return type and body locals, in order.
    let mut new_methods = Vec::with_capacity(types[index].methods.len());
    for method in &types[index].methods {
        let mut method_env = env.clone();
        let mut new_method = method.clone();
        for param in &mut new_method.params {
            if let Some(ty) = &mut param.ty {
                let resolved = resolve_typeof_in_type(ty, &method_env, types);
                method_env.insert(param.name.clone(), resolved.clone());
                *ty = resolved;
            }
        }
        if let Some(return_type) = &mut new_method.return_type {
            *return_type = resolve_typeof_in_type(return_type, &method_env, types);
        }
        for statement in &mut new_method.body {
            resolve_statement_typeof(statement, &mut method_env, types);
        }
        new_methods.push(new_method);
    }
    types[index].methods = new_methods;
}

/// Resolve the placeholders inside one type position recursively.
fn resolve_typeof_in_type(
    ty: &crate::ast::TypeExpr,
    env: &BTreeMap<String, crate::ast::TypeExpr>,
    types: &[ElaboratedType],
) -> crate::ast::TypeExpr {
    match ty {
        crate::ast::TypeExpr::TypeOf { expr, .. } => {
            static_type_of_expr(expr, env, types).unwrap_or_else(|| ty.clone())
        }
        crate::ast::TypeExpr::TypeOfItem { expr, .. } => {
            match static_type_of_expr(expr, env, types) {
                Some(crate::ast::TypeExpr::List { element, .. }) => *element,
                _ => ty.clone(),
            }
        }
        crate::ast::TypeExpr::List { element, key, span } => crate::ast::TypeExpr::List {
            element: Box::new(resolve_typeof_in_type(element, env, types)),
            key: key.clone(),
            span: *span,
        },
        crate::ast::TypeExpr::WhenQualified { value, base, span } => {
            crate::ast::TypeExpr::WhenQualified {
                value: value.clone(),
                base: Box::new(resolve_typeof_in_type(base, env, types)),
                span: *span,
            }
        }
        crate::ast::TypeExpr::TemplateRef { name, args, span } => {
            crate::ast::TypeExpr::TemplateRef {
                name: name.clone(),
                args: args
                    .iter()
                    .map(|arg| resolve_typeof_in_type(arg, env, types))
                    .collect(),
                span: *span,
            }
        }
        other => other.clone(),
    }
}

/// Resolve the placeholders inside one statement's type positions
/// (local-variable declarations), in statement order: a later `var k:
/// typeof(x)` sees `x`'s resolved type. The locals share one flat scope,
/// like the checker's model.
fn resolve_statement_typeof(
    statement: &mut crate::ast::Stmt,
    env: &mut BTreeMap<String, crate::ast::TypeExpr>,
    types: &[ElaboratedType],
) {
    match statement {
        crate::ast::Stmt::VarDecl {
            name, ty: Some(ty), ..
        } => {
            let resolved = resolve_typeof_in_type(ty, env, types);
            env.insert(name.clone(), resolved.clone());
            *ty = resolved;
        }
        crate::ast::Stmt::If {
            then_body,
            else_body,
            ..
        } => {
            for inner in then_body {
                resolve_statement_typeof(inner, env, types);
            }
            if let Some(else_body) = else_body {
                for inner in else_body {
                    resolve_statement_typeof(inner, env, types);
                }
            }
        }
        crate::ast::Stmt::While { body, .. } => {
            for inner in body {
                resolve_statement_typeof(inner, env, types);
            }
        }
        crate::ast::Stmt::RepeatUntil { body, .. } => {
            for inner in body {
                resolve_statement_typeof(inner, env, types);
            }
        }
        crate::ast::Stmt::ForEach { body, .. } => {
            for inner in body {
                resolve_statement_typeof(inner, env, types);
            }
        }
        crate::ast::Stmt::ForTo { body, .. } => {
            for inner in body {
                resolve_statement_typeof(inner, env, types);
            }
        }
        crate::ast::Stmt::Case { arms, default, .. } => {
            for (_, body) in arms {
                for inner in body {
                    resolve_statement_typeof(inner, env, types);
                }
            }
            if let Some(default) = default {
                for inner in default {
                    resolve_statement_typeof(inner, env, types);
                }
            }
        }
        crate::ast::Stmt::Check { else_body, .. } => {
            for inner in else_body {
                resolve_statement_typeof(inner, env, types);
            }
        }
        _ => {}
    }
}

/// The static type of a `typeof()`/`typeof_item()` argument (5.9)
/// against the composed types: a field of the enclosing type, a local
/// `var`, a method parameter, a literal, or a member path through a
/// struct-typed name. The expression is only inspected, never evaluated.
fn static_type_of_expr(
    expr: &crate::ast::Expr,
    env: &BTreeMap<String, crate::ast::TypeExpr>,
    types: &[ElaboratedType],
) -> Option<crate::ast::TypeExpr> {
    match expr {
        crate::ast::Expr::Ident { name, .. } => env.get(name).cloned(),
        crate::ast::Expr::Paren { expr, .. } => static_type_of_expr(expr, env, types),
        crate::ast::Expr::MemberAccess { object, name, .. } => {
            let base = static_type_of_expr(object, env, types)?;
            let type_name = struct_like_name(&base)?;
            let ty = types.iter().find(|ty| ty.name == type_name)?;
            ty.fields
                .iter()
                .find(|field| &field.name == name)
                .map(|field| field.ty.clone())
        }
        // A literal's static type is its predefined scalar base; the
        // unsized and sized numeric literals are `int` at this stage.
        crate::ast::Expr::IntLiteral { .. } | crate::ast::Expr::BitLiteral { .. } => {
            Some(scalar_type_of(crate::ast::ScalarBase::Int))
        }
        crate::ast::Expr::RealLiteral { .. } => Some(scalar_type_of(crate::ast::ScalarBase::Real)),
        crate::ast::Expr::BoolLiteral { .. } => Some(scalar_type_of(crate::ast::ScalarBase::Bool)),
        crate::ast::Expr::StringLiteral { .. } => {
            Some(scalar_type_of(crate::ast::ScalarBase::String))
        }
        _ => None,
    }
}

fn struct_like_name(ty: &crate::ast::TypeExpr) -> Option<String> {
    match ty {
        crate::ast::TypeExpr::Named { name, .. } => Some(name.clone()),
        crate::ast::TypeExpr::WhenQualified { base, .. } => struct_like_name(base),
        _ => None,
    }
}

fn scalar_type_of(base: crate::ast::ScalarBase) -> crate::ast::TypeExpr {
    crate::ast::TypeExpr::Scalar {
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
