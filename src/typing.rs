//! The conservative per-expression type check over the elaborated
//! types — the first stage of the type system the STATUS page lists as
//! missing ("full type checking and name resolution"). This is **not**
//! a full type system: it infers only what a fail-open reading of an
//! expression can pin down, and it reports only *definite* mismatches —
//! anything inferable-but-uncertain stays `Unknown` and every rule
//! stays silent. No false positives by construction: a construct the
//! checker cannot pin down produces no diagnostic anywhere downstream.
//!
//! What it checks:
//! - assignment / `var` init / argument passing / `return` value
//!   against a known target type — a definite value kind against a
//!   definite target kind it cannot be (E0220);
//! - a member (field or method) that does not exist on a known
//!   receiver type with no `when` views (when-only members exist
//!   conditionally, so a type with `when` views stays silent — E0221);
//! - indexing a definitely non-list value (E0222);
//! - an operator applied to definitely incompatible operand kinds
//!   (string against a number, an object in arithmetic — E0223).
//!
//! Declared parameter kinds are visible in their method bodies, and
//! `var x := expression` keeps an inferable initializer kind (clause 18).
//! Homogeneous list literals and equal-kind ternary branches propagate
//! kinds; ambiguous expressions and untyped parameters stay unknown.
//! Scalar/list aliases resolve with a recursion bound; custom numeric
//! dispatch remains outside this lattice.
//!
//! The pass walks methods, field init values, `keep` constraints,
//! `on` bodies and coverage item expressions, including synthesized
//! `when` views. Effective Boolean coverage filters, visibility and
//! ignore/illegal predicates use their owner/source/unit binding scopes.
//! Define kinds and composite inputs remain unknown; other option types
//! are not checked here. Structural option syntax and unit scopes are
//! checked during elaboration;
//! `table`/`table_row` receivers and constructs the checker cannot pin
//! down also stay unchecked. The pass
//! runs over composed types (post `like`/`extend` fold), so extended
//! members are visible. Nested value operands, scoped evaluate/new bodies,
//! statement value children and temporal rule/wait values are traversed.
//! Explicit evaluate, slice and table-test results retain known kinds;
//! qualified static field/call signatures retain declared kinds. Static
//! bodies have a declaring namespace without a me receiver and are checked
//! once from their registries. Omitted extension annotations inherit from
//! their declaration. Cast results, implicit contexts, unregistered statics
//! and type-embedded expressions remain conservative. Index-kind/membership compatibility
//! and conditional is-a bindings still need separate typing rules.
//!
//! ## Diagnostic codes
//!
//! - `E0220` a definite value stored into a definite target it cannot
//!   be (assignment, `var` init, argument, `return`)
//! - `E0221` a member that does not exist on a known, when-less
//!   receiver type
//! - `E0222` indexing a definitely non-list value
//! - `E0223` an operator on definitely incompatible operand kinds

use crate::ast::{DeclarationKind, Expr, MethodDecl, ScalarBase, Stmt, TypeExpr, UnaryOp};
use crate::diagnostic::Diagnostic;
use crate::elab::{Elaborated, ElaboratedType};
use std::collections::BTreeMap;

/// The conservative type lattice. `Unknown` carries no opinion and
/// makes every rule about it silent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    Unknown,
    Void,
    Bool,
    Int,
    Real,
    Str,
    /// An enumerand of the named enumeral type; numerically it
    /// coerces to an integer.
    Enum(String),
    /// A list of the element kind (the element may be `Unknown`).
    List(Box<Ty>),
    /// A struct/unit/interface value (the name is the canonical
    /// composed name; a template instance names the instance).
    Object(String),
    /// A clause 33 resource-sharing handle.
    Sync,
    /// An `instance_iterator` handle.
    Iterator,
}

impl Ty {
    /// Render for the diagnostic message.
    fn name(&self) -> String {
        match self {
            Ty::Unknown => "unknown".to_owned(),
            Ty::Void => "NULL".to_owned(),
            Ty::Bool => "bool".to_owned(),
            Ty::Int => "int".to_owned(),
            Ty::Real => "real".to_owned(),
            Ty::Str => "string".to_owned(),
            Ty::Enum(type_name) => type_name.clone(),
            Ty::List(element) => format!("list of {}", element.name()),
            Ty::Object(name) => name.clone(),
            Ty::Sync => "resource-sharing object".to_owned(),
            Ty::Iterator => "instance_iterator".to_owned(),
        }
    }
}

/// Whether a type carries an opinion: a definite kind, only the NULL
/// reference (which unifies with object kinds), or no opinion.
#[derive(PartialEq, Eq, Clone, Copy)]
enum Opinion {
    Yes,
    Null,
    No,
}

fn opinion(ty: &Ty) -> Opinion {
    match ty {
        Ty::Unknown => Opinion::No,
        Ty::Void => Opinion::Null,
        _ => Opinion::Yes,
    }
}

fn has_opinion(ty: &Ty) -> bool {
    matches!(opinion(ty), Opinion::Yes | Opinion::Null)
}

/// The numeric family: enumerals coerce to integers, integers promote
/// to reals (the 5.6.3 precision contexts).
fn numeric(ty: &Ty) -> bool {
    matches!(ty, Ty::Int | Ty::Real | Ty::Enum(_))
}

/// A kind shared by both alternatives. An unknown or differing kind
/// stays unknown: this is declaration inference, not union typing.
fn shared_kind(left: Ty, right: Ty) -> Ty {
    if left == right { left } else { Ty::Unknown }
}

/// Whether `value` may be stored into `target`. `Unknown` on either
/// side is compatible by definition (no opinion); NULL unifies with
/// object kinds (the null reference); the numeric family mixes.
fn compatible(value: &Ty, target: &Ty, types: &[ElaboratedType]) -> bool {
    if !has_opinion(value) || !has_opinion(target) {
        return true;
    }
    if matches!(value, Ty::Void) || matches!(target, Ty::Void) {
        // The null reference unifies with object kinds only; NULL into
        // a scalar target stays the mismatch below.
        return matches!(
            (value, target),
            (Ty::Void, Ty::Object(_)) | (Ty::Object(_), Ty::Void)
        );
    }
    match (value, target) {
        (a, b) if a == b => true,
        (a, b) if numeric(a) && numeric(b) => true,
        (Ty::List(a), Ty::List(b)) => compatible(a, b, types),
        (Ty::Object(value_name), Ty::Object(target_name)) => {
            object_assignable(value_name, target_name, types)
        }
        _ => false,
    }
}

/// Whether a value of struct `value_name` is assignable to a slot of
/// struct `target_name`: the same type, the target in the value's
/// `like` chain or implemented interfaces, or an interface-typed
/// target whose implementing struct is the value's type (6.13).
fn object_assignable(value_name: &str, target_name: &str, types: &[ElaboratedType]) -> bool {
    if value_name == target_name {
        return true;
    }
    let find = |name: &str| types.iter().find(|t| t.name == name);
    // Walk up from the value: the target as the value itself, a like
    // ancestor, or an implemented interface.
    let mut chain = value_name.to_owned();
    let mut seen = std::collections::BTreeSet::new();
    while let Some(current) = find(&chain) {
        if current.name == target_name || current.implements.iter().any(|i| i == target_name) {
            return true;
        }
        match &current.base {
            Some(base) if seen.insert(chain.clone()) => chain = base.clone(),
            _ => break,
        }
    }
    // The reverse: the value is an ancestor of the target (assignment
    // of a base-typed value into a derived-typed slot is not legal,
    // but the checker treats the like relation symmetrically — the
    // runtime's binding rules are the authority; this keeps the
    // checker from arguing where the runtime would accept a cast).
    let mut chain = target_name.to_owned();
    let mut seen = std::collections::BTreeSet::new();
    while let Some(current) = find(&chain) {
        if current.name == value_name || current.implements.iter().any(|i| i == value_name) {
            return true;
        }
        match &current.base {
            Some(base) if seen.insert(chain.clone()) => chain = base.clone(),
            _ => break,
        }
    }
    false
}

/// The 2019 clause 33 sync type names — the `new` construction and
/// type positions map them to the sync value kind.
fn sync_type(name: &str) -> bool {
    matches!(name, "semaphore" | "rdv_semaphore" | "locker")
}

/// The predefined members the runtime accepts on any instance without a
/// per-type declaration: the 7.4 any_unit queries, 23's `hdl_path()`,
/// the 9.7.3 port attributes and the 7.3.4/7.3.5 unit attributes. The
/// unknown-member rule stays silent for them (they resolve at
/// elaboration or in the runtime's predefined dispatch).
const PREDEFINED_MEMBERS: &[&str] = &[
    "e_path",
    "get_children",
    "get_ports",
    "get_ports_recursively",
    "hdl_path",
    "delayed",
    "edge",
    "pack_options",
    "per_inst_cover",
];

/// The statement/expression checker for one method body.
struct TypeChecker<'a> {
    elaborated: &'a Elaborated,
    /// A static method/on block has an owner namespace but no me receiver.
    static_context: Option<&'a str>,
    types: &'a [ElaboratedType],
    /// Enumerand → its enumeral type name (composed `type` kinds).
    enumeral_table: BTreeMap<String, String>,
    /// The scoped local variables; the last scope is innermost.
    locals: Vec<BTreeMap<String, Ty>>,
    /// The enclosing method's return type.
    return_ty: Option<&'a TypeExpr>,
    /// The enclosing composed type (the `me`/`self` receiver).
    me_type: Option<&'a str>,
    diagnostics: Vec<Diagnostic>,
    /// Runtime lookup resolves defines before locals and fields. Their
    /// value kinds stay unknown until constant typing is complete.
    defines: &'a BTreeMap<String, Expr>,
}

impl<'a> TypeChecker<'a> {
    fn new(elaborated: &'a Elaborated) -> Self {
        let types = &elaborated.types;
        let defines = &elaborated.defines;
        let mut enumeral_table = BTreeMap::new();
        for t in types {
            if t.kind == DeclarationKind::Type {
                for enumeral in &t.enumerals {
                    enumeral_table.insert(enumeral.name.clone(), t.name.clone());
                }
            }
        }
        Self {
            elaborated,
            static_context: None,
            types,
            enumeral_table,
            locals: vec![BTreeMap::new()],
            return_ty: None,
            me_type: None,
            diagnostics: Vec::new(),
            defines,
        }
    }

    fn error(&self, code: &'static str, message: String, span: crate::ast::Span) -> Diagnostic {
        Diagnostic::new(code, message, span.0, span.1)
    }

    fn push_scope(&mut self) {
        self.locals.push(BTreeMap::new());
    }

    fn pop_scope(&mut self) {
        self.locals.pop();
    }

    fn bind_local(&mut self, name: &str, ty: Ty) {
        if let Some(scope) = self.locals.last_mut() {
            scope.insert(name.to_owned(), ty);
        }
    }

    fn local(&self, name: &str) -> Option<&Ty> {
        self.locals.iter().rev().find_map(|scope| scope.get(name))
    }

    /// The declared kind of a `me` field.
    fn me_field(&self, name: &str) -> Option<Ty> {
        let type_name = self.me_type?;
        let t = self.types.iter().find(|t| t.name == type_name)?;
        t.fields
            .iter()
            .find(|field| field.name == name)
            .map(|field| self.type_of(&field.ty))
    }

    fn find_type(&self, name: &str) -> Option<&'a ElaboratedType> {
        self.types.iter().find(|t| t.name == name)
    }

    /// The conservative conversion of a declared type expression.
    fn type_of(&self, ty: &TypeExpr) -> Ty {
        self.type_of_at_depth(ty, 0)
    }

    fn type_of_at_depth(&self, ty: &TypeExpr, depth: usize) -> Ty {
        // Recursive aliases and unusually deep type expressions must
        // stay fail-open instead of overflowing the checker stack.
        if depth >= 64 {
            return Ty::Unknown;
        }
        match ty {
            TypeExpr::Scalar { base, .. } => match base {
                ScalarBase::Bool => Ty::Bool,
                ScalarBase::Real => Ty::Real,
                ScalarBase::String => Ty::Str,
                _ => Ty::Int,
            },
            TypeExpr::List { element, .. } => {
                Ty::List(Box::new(self.type_of_at_depth(element, depth + 1)))
            }
            TypeExpr::Named { name, .. } => {
                if sync_type(name) {
                    return Ty::Sync;
                }
                if name == "instance_iterator" {
                    return Ty::Iterator;
                }
                if self.enumeral_table.values().any(|t| t == name) {
                    // An enumeral type.
                    return Ty::Enum(name.clone());
                }
                if let Some(declared) = self.find_type(name) {
                    if declared.kind == DeclarationKind::NumericType {
                        // Custom numeric operator dispatch is outside
                        // this lattice; do not treat it as an object.
                        return Ty::Unknown;
                    }
                    if let Some(underlying) = &declared.underlying {
                        return self.type_of_at_depth(underlying, depth + 1);
                    }
                }
                Ty::Object(name.clone())
            }
            TypeExpr::WhenQualified { base, .. } => self.type_of_at_depth(base, depth + 1),
            TypeExpr::InlineEnum { .. } => Ty::Enum("<inline enumeral list>".to_owned()),
            // Template/formal/typeof spellings stay unchecked (v1).
            _ => Ty::Unknown,
        }
    }

    /// Report a definite mismatch, or nothing when either side has no
    /// opinion.
    fn check_assignable(&mut self, value: &Ty, target: &Ty, span: crate::ast::Span, what: &str) {
        if !compatible(value, target, self.types) {
            self.diagnostics.push(self.error(
                "E0220",
                format!(
                    "{what}: a value of kind '{}' cannot be stored into '{}'",
                    value.name(),
                    target.name()
                ),
                span,
            ));
        }
    }

    // ----------------------------------------------------------------
    // Statements
    // ----------------------------------------------------------------

    fn check_body(&mut self, body: &[Stmt]) {
        self.push_scope();
        for statement in body {
            self.check_statement(statement);
        }
        self.pop_scope();
    }

    fn check_statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::VarDecl { name, ty, init, .. } => {
                let declared = ty.as_ref().map(|ty| self.type_of(ty));
                let inferred = if let Some(init) = init {
                    let value = self.infer(init);
                    if let Some(target) = &declared {
                        self.check_assignable(
                            &value,
                            target,
                            init.span(),
                            "variable initialization",
                        );
                    }
                    // NULL alone does not identify the declaration's
                    // reference type; a later assignment cannot narrow it.
                    if value == Ty::Void {
                        Ty::Unknown
                    } else {
                        value
                    }
                } else {
                    Ty::Unknown
                };
                self.bind_local(name, declared.unwrap_or(inferred));
            }
            Stmt::Assign { target, value, .. } | Stmt::RgfAssign { target, value, .. } => {
                let value_ty = self.infer(value);
                let target_ty = self.infer(target);
                self.check_assignable(&value_ty, &target_ty, value.span(), "assignment");
            }
            Stmt::ExprStmt { expr, .. } => {
                self.infer(expr);
            }
            Stmt::Return { value, .. } => {
                let Some(value) = value else {
                    return;
                };
                let value_ty = self.infer(value);
                if let Some(return_ty) = self.return_ty {
                    let target = self.type_of(return_ty);
                    self.check_assignable(&value_ty, &target, value.span(), "return value");
                }
            }
            Stmt::If {
                cond,
                then_body,
                else_body,
                ..
            } => {
                self.infer(cond);
                self.check_body(then_body);
                if let Some(else_body) = else_body {
                    self.check_body(else_body);
                }
            }
            Stmt::While { cond, body, .. } => {
                self.infer(cond);
                self.check_body(body);
            }
            Stmt::RepeatUntil { body, cond, .. } => {
                self.check_body(body);
                self.infer(cond);
            }
            Stmt::Check {
                condition,
                else_body,
                ..
            } => {
                self.infer(condition);
                self.check_body(else_body);
            }
            Stmt::ForEach {
                var,
                index_var,
                list,
                body,
                ..
            } => {
                let element = match self.infer(list) {
                    Ty::List(element) => *element,
                    _ => Ty::Unknown,
                };
                self.push_scope();
                // The default element binding is `it`.
                self.bind_local(var.as_deref().unwrap_or("it"), element);
                if let Some(index_var) = index_var {
                    self.bind_local(index_var, Ty::Int);
                }
                self.check_body(body);
                self.pop_scope();
            }
            Stmt::ForTo {
                var,
                from,
                to,
                body,
                ..
            } => {
                self.infer(from);
                self.infer(to);
                self.push_scope();
                self.bind_local(var, Ty::Int);
                self.check_body(body);
                self.pop_scope();
            }
            Stmt::Case {
                scrutinee,
                arms,
                default,
                ..
            } => {
                self.infer(scrutinee);
                self.push_scope();
                for (matchers, body) in arms {
                    for matcher in matchers {
                        self.infer(matcher);
                    }
                    self.check_body(body);
                }
                if let Some(default) = default {
                    self.check_body(default);
                }
                self.pop_scope();
            }
            Stmt::Print { exprs, .. } => {
                for expr in exprs {
                    self.infer(expr);
                }
            }
            Stmt::Gen { expr, .. } => {
                self.infer(expr);
            }
            Stmt::Wait { cycles, .. } => {
                self.infer(cycles);
            }
            Stmt::Emit { target, .. } => {
                self.infer(target);
            }
            Stmt::Start { call, .. } | Stmt::Compute { call, .. } => {
                self.infer(call);
            }
            Stmt::WaitTemporal { te, .. } => self.check_temporal(te),
            Stmt::Sync { te, .. } => {
                if let Some(te) = te {
                    self.check_temporal(te);
                }
            }
            // These nodes carry no expression children.
            Stmt::StopRun { .. }
            | Stmt::Break { .. }
            | Stmt::Continue { .. }
            | Stmt::SeqDo { .. } => {}
        }
    }

    // ----------------------------------------------------------------
    // Expressions
    // ----------------------------------------------------------------

    /// Infer the conservative kind of an expression, reporting the
    /// member/index/operator rules along the way.
    fn infer(&mut self, expr: &Expr) -> Ty {
        match expr {
            Expr::IntLiteral { .. } | Expr::BitLiteral { .. } => Ty::Int,
            Expr::RealLiteral { .. } => Ty::Real,
            Expr::BoolLiteral { .. } => Ty::Bool,
            Expr::StringLiteral { .. } => Ty::Str,
            Expr::Ident { name, .. } => self.infer_ident(name),
            Expr::Paren { expr, .. } => self.infer(expr),
            Expr::MemberAccess {
                object, name, span, ..
            } => {
                let object_ty = self.infer(object);
                self.infer_member(&object_ty, name, *span)
            }
            Expr::QualifiedField { base, field, .. } => {
                let object_ty = self.infer(base);
                self.infer_member(&object_ty, field, base.span())
            }
            Expr::Call { callee, args, span } => self.infer_call(callee, args, *span),
            Expr::Index { base, index, span } => {
                let base_ty = self.infer(base);
                self.infer(index);
                if let Ty::List(element) = base_ty {
                    *element
                } else if has_opinion(&base_ty) {
                    self.diagnostics.push(self.error(
                        "E0222",
                        format!(
                            "indexing a value of kind '{}' — only a list is indexable",
                            base_ty.name()
                        ),
                        *span,
                    ));
                    Ty::Unknown
                } else {
                    Ty::Unknown
                }
            }
            Expr::Binary { op, lhs, rhs, span } => self.infer_binary(*op, lhs, rhs, *span),
            Expr::Unary { op, expr, span } => self.infer_unary(*op, expr, *span),
            Expr::New { ty, inits, .. } => {
                let result = match ty {
                    Some(TypeExpr::Named { name, .. }) if sync_type(name) => Ty::Sync,
                    Some(TypeExpr::Named { name, .. }) if name == "instance_iterator" => {
                        Ty::Iterator
                    }
                    Some(TypeExpr::Named { name, .. }) => Ty::Object(name.clone()),
                    Some(TypeExpr::TemplateRef { name, args, .. }) => {
                        Ty::Object(crate::ast::template_instance_name(name, args))
                    }
                    _ => Ty::Unknown,
                };
                // The runtime's with block has a fresh frame and the new
                // object as me. Caller locals and return types do not apply.
                let old_me = self.me_type;
                let old_static = self.static_context.take();
                let old_return = self.return_ty.take();
                let old_locals = std::mem::take(&mut self.locals);
                self.me_type = match &result {
                    Ty::Object(name) => self.find_type(name).map(|ty| ty.name.as_str()),
                    _ => None,
                };
                self.check_body(inits);
                self.locals = old_locals;
                self.return_ty = old_return;
                self.me_type = old_me;
                self.static_context = old_static;
                result
            }
            Expr::ListConcat { items, .. } => {
                let mut element = None;
                for item in items {
                    let kind = self.infer(item);
                    element = Some(match element {
                        None => kind,
                        Some(previous) => shared_kind(previous, kind),
                    });
                }
                Ty::List(Box::new(element.unwrap_or(Ty::Unknown)))
            }
            Expr::Ternary {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                self.infer(cond);
                let left = self.infer(then_expr);
                let right = self.infer(else_expr);
                shared_kind(left, right)
            }
            Expr::InSet { element, set, .. } => {
                self.infer(element);
                self.check_set(set);
                Ty::Bool
            }
            Expr::IsA { expr, .. } => {
                self.infer(expr);
                // Conditional name bindings still require flow-sensitive
                // resolution; this traversal does not narrow the operand.
                Ty::Bool
            }
            Expr::CastAs { expr, .. } => {
                self.infer(expr);
                // Several runtime casts preserve the input value. Do not
                // assume that the spelled target determines its value kind.
                Ty::Unknown
            }
            Expr::BitSlice {
                base, high, low, ..
            } => {
                self.infer(base);
                self.infer(high);
                self.infer(low);
                Ty::Int
            }
            Expr::InTable { tuple, rows, .. } => {
                for expr in tuple {
                    self.infer(expr);
                }
                for row in rows {
                    for item in row {
                        match item {
                            crate::ast::TableElem::Scalar(expr) => {
                                self.infer(expr);
                            }
                            crate::ast::TableElem::Set(set) => self.check_set(set),
                            crate::ast::TableElem::TypeSet { .. } => {}
                        }
                    }
                }
                Ty::Bool
            }
            Expr::SetLiteral { items, .. } => {
                self.check_set(items);
                Ty::Unknown // Set kinds are outside the current lattice.
            }
            Expr::ChosenFrom { target, list, .. } => {
                self.infer(target);
                self.infer(list);
                Ty::Unknown // A generation directive, not an ordinary value.
            }
            Expr::Select { arms, .. } => {
                for arm in arms {
                    self.infer(&arm.weight);
                }
                // Choice tokens include a separate distribution grammar.
                Ty::Unknown
            }
            Expr::Evaluate { ty, var, body, .. } => {
                let result = ty
                    .as_ref()
                    .map(|ty| self.type_of(ty))
                    .unwrap_or(Ty::Unknown);
                self.push_scope();
                self.bind_local(var.as_deref().unwrap_or("value"), result.clone());
                self.check_body(body);
                self.pop_scope();
                result
            }
            Expr::StaticAccess {
                type_name, member, ..
            } => self.static_field(type_name, member).unwrap_or(Ty::Unknown),
            // Leaves without ordinary expression children. Type-embedded
            // expressions still need their own scopes.
            Expr::Formal { .. }
            | Expr::HdlPath { .. }
            | Expr::AnnotationTypeRef { .. }
            | Expr::SetOfValues { .. } => Ty::Unknown,
        }
    }

    fn check_set(&mut self, set: &crate::ast::InSetExpr) {
        match set {
            crate::ast::InSetExpr::Items(items) => {
                for item in items {
                    self.infer(item);
                }
            }
            crate::ast::InSetExpr::Range(low, high) => {
                self.infer(low);
                self.infer(high);
            }
            crate::ast::InSetExpr::Expr(expr) => {
                self.infer(expr);
            }
        }
    }

    /// Traverse temporal value children without inventing an event type or
    /// imposing a new temporal operand rule (clauses 11-13).
    fn check_temporal(&mut self, te: &crate::ast::TemporalExpr) {
        use crate::ast::TemporalExpr;
        match te {
            TemporalExpr::True { expr } | TemporalExpr::EdgeSample { expr, .. } => {
                self.infer(expr);
            }
            TemporalExpr::Repeat { low, high, te, .. } => {
                if let Some(low) = low {
                    self.infer(low);
                }
                // ~[n] stores one written expression as both bounds.
                if high != low
                    && let Some(high) = high
                {
                    self.infer(high);
                }
                self.check_temporal(te);
            }
            TemporalExpr::Sequence { items } => {
                for item in items {
                    self.check_temporal(item);
                }
            }
            TemporalExpr::Yield { lhs, rhs }
            | TemporalExpr::And(lhs, rhs)
            | TemporalExpr::Or(lhs, rhs)
            | TemporalExpr::Sampled {
                te: lhs,
                event: rhs,
            } => {
                self.check_temporal(lhs);
                self.check_temporal(rhs);
            }
            TemporalExpr::Not(te)
            | TemporalExpr::Fail(te)
            | TemporalExpr::Eventually(te)
            | TemporalExpr::Group(te) => self.check_temporal(te),
            TemporalExpr::EventRef { .. } | TemporalExpr::Cycle => {}
        }
    }

    fn check_coverage_boolean(&mut self, expr: Option<&Expr>, option: &str) {
        if let Some(expr) = expr {
            let value = self.infer(expr);
            self.check_assignable(
                &value,
                &Ty::Bool,
                expr.span(),
                &format!("coverage {option}"),
            );
        }
    }

    fn infer_ident(&mut self, name: &str) -> Ty {
        if name == "me" || name == "self" {
            return match self.me_type {
                Some(type_name) => Ty::Object(type_name.to_owned()),
                None => Ty::Unknown,
            };
        }
        if name.ends_with('$') || self.defines.contains_key(name) {
            return Ty::Unknown;
        }
        if matches!(name, "TEST_DONE" | "MAX_INT" | "MIN_INT" | "MAX_UINT") {
            return Ty::Int;
        }
        if name == "NULL" {
            return Ty::Void;
        }
        if let Some(ty) = self.local(name) {
            return ty.clone();
        }
        if let Some(ty) = self.me_field(name) {
            return ty;
        }
        if let Some(owner) = self.me_type.or(self.static_context)
            && let Some(kind) = self.bare_static_field(owner, name)
        {
            return kind;
        }
        if let Some(type_name) = self.enumeral_table.get(name) {
            return Ty::Enum(type_name.clone());
        }
        Ty::Unknown
    }

    fn static_field(&self, owner: &str, name: &str) -> Option<Ty> {
        self.elaborated
            .static_fields
            .get(owner)?
            .iter()
            .find(|field| field.name == name)
            .map(|field| self.type_of(&field.ty))
    }

    fn bare_static_field(&self, owner: &str, name: &str) -> Option<Ty> {
        let mut owner = owner;
        for _ in 0..64 {
            if let Some(kind) = self.static_field(owner, name) {
                return Some(kind);
            }
            owner = self.find_type(owner)?.base.as_deref()?;
        }
        None
    }

    /// Prefer the declaration over an extension that precedes it in the file.
    fn static_method(&self, owner: &str, name: &str) -> Option<&'a MethodDecl> {
        let methods = self.elaborated.static_methods.get(owner)?;
        methods
            .iter()
            .find(|method| method.name == name && method.modifier.is_none())
            .or_else(|| methods.iter().find(|method| method.name == name))
    }

    /// A field or method name on a known receiver kind: the field's
    /// declared kind, or a known method's return kind (a method name
    /// read as a value falls to Unknown — it is rarely intended).
    fn infer_member(&mut self, object_ty: &Ty, name: &str, span: crate::ast::Span) -> Ty {
        let Ty::Object(type_name) = object_ty else {
            return Ty::Unknown;
        };
        // The `path$` tick access (clause 23) and any other sigil
        // spelling is not a member lookup.
        if name.contains('$') {
            return Ty::Unknown;
        }
        let Some(t) = self.find_type(type_name) else {
            return Ty::Unknown;
        };
        if let Some(field) = t.fields.iter().find(|field| field.name == name) {
            return self.type_of(&field.ty);
        }
        if let Some(method) = t.methods.iter().find(|m| m.name == name) {
            return method
                .return_type
                .as_ref()
                .map(|rt| self.type_of(rt))
                .unwrap_or(Ty::Void);
        }
        if t.events.iter().any(|event| event.name == name)
            || t.ports.iter().any(|port| port.name == name)
        {
            return Ty::Unknown;
        }
        // When-only members exist conditionally; a type with `when`
        // views stays silent on unknown members.
        if !t.whens.is_empty() {
            return Ty::Unknown;
        }
        // The predefined any-instance members are not declared per type.
        if PREDEFINED_MEMBERS.contains(&name) {
            return Ty::Unknown;
        }
        self.diagnostics.push(self.error(
            "E0221",
            format!("'{type_name}' has no field or method '{name}'"),
            span,
        ));
        Ty::Unknown
    }

    fn infer_call(&mut self, callee: &Expr, args: &[Expr], _span: crate::ast::Span) -> Ty {
        // Builtin routines: `out`, `append`, `pack`, ... — the runtime
        // dispatches by name; the checker stays out of their argument
        // shapes (E0402 at run time is the authority there).
        if let Expr::Ident { name, .. } = callee
            && crate::builtins::is_builtin_routine(name)
        {
            for arg in args {
                self.infer(arg);
            }
            return match name.as_str() {
                "append" => Ty::Str,
                "out" | "outf" | "dut_error" | "error" | "message" | "stop_run"
                | "raise_objection" | "drop_objection" | "set_check" | "do_bind" => Ty::Void,
                "pack" | "swap" | "hdl_get" => Ty::Int,
                _ => Ty::Unknown,
            };
        }
        match callee {
            // A bare method call on `me`.
            Expr::Ident { name, .. } => {
                let me_name = self.me_type.map(str::to_owned);
                let method = me_name
                    .as_deref()
                    .and_then(|type_name| self.find_type(type_name))
                    .and_then(|t| t.methods.iter().find(|m| m.name.as_str() == name));
                let Some(method) = method else {
                    for arg in args {
                        self.infer(arg);
                    }
                    return Ty::Unknown;
                };
                let return_ty = method
                    .return_type
                    .as_ref()
                    .map(|rt| self.type_of(rt))
                    .unwrap_or(Ty::Void);
                let argument_types: Vec<_> = args.iter().map(|arg| self.infer(arg)).collect();
                self.check_args(method, args, &argument_types);
                return_ty
            }
            Expr::MemberAccess {
                object,
                name,
                span: member_span,
                ..
            } => {
                let object_ty = self.infer(object);
                if let Ty::List(element) = &object_ty
                    && matches!(
                        name.as_str(),
                        "min"
                            | "max"
                            | "min_index"
                            | "max_index"
                            | "has"
                            | "count"
                            | "all"
                            | "all_indices"
                    )
                {
                    self.push_scope();
                    self.bind_local("it", element.as_ref().clone());
                    for arg in args {
                        let argument_ty = self.infer(arg);
                        if matches!(name.as_str(), "has" | "count" | "all" | "all_indices") {
                            self.check_assignable(
                                &argument_ty,
                                &Ty::Bool,
                                arg.span(),
                                "list predicate",
                            );
                        }
                    }
                    self.pop_scope();
                    return match name.as_str() {
                        "min_index" | "max_index" | "count" => Ty::Int,
                        "has" => Ty::Bool,
                        "all_indices" => Ty::List(Box::new(Ty::Int)),
                        "all" => object_ty,
                        _ => element.as_ref().clone(),
                    };
                }
                let argument_types: Vec<_> = args.iter().map(|arg| self.infer(arg)).collect();
                match &object_ty {
                    Ty::Object(_) => {
                        // The receiver's method call: check the
                        // arguments against the composed signature.
                        let t = self.find_type(type_name_of(&object_ty));
                        if let Some(method) =
                            t.and_then(|t| t.methods.iter().find(|m| m.name.as_str() == name))
                        {
                            self.check_args(method, args, &argument_types);
                        }
                        self.infer_member(&object_ty, name, *member_span)
                    }
                    Ty::List(_) => match name.as_str() {
                        "size" | "max_index" => Ty::Int,
                        "is_empty" | "has" | "all_equal" => Ty::Bool,
                        _ => Ty::Unknown,
                    },
                    Ty::Sync => match name.as_str() {
                        "get_value" | "get_max_value" => Ty::Int,
                        "try_up" | "try_down" => Ty::Bool,
                        _ => Ty::Void,
                    },
                    Ty::Str | Ty::Int | Ty::Real | Ty::Bool | Ty::Enum(_) => match name.as_str() {
                        "to_string" => Ty::Str,
                        _ => Ty::Unknown,
                    },
                    _ => Ty::Unknown,
                }
            }
            // A subtype-qualified call (`p.red'f()`) — the field
            // resolution is the plain one (4.17.3).
            Expr::QualifiedField { base, field, .. } => {
                let object_ty = self.infer(base);
                let argument_types: Vec<_> = args.iter().map(|arg| self.infer(arg)).collect();
                if let Ty::Object(_) = &object_ty {
                    let t = self.find_type(type_name_of(&object_ty));
                    if let Some(method) =
                        t.and_then(|t| t.methods.iter().find(|m| m.name.as_str() == field))
                    {
                        self.check_args(method, args, &argument_types);
                        return method
                            .return_type
                            .as_ref()
                            .map(|rt| self.type_of(rt))
                            .unwrap_or(Ty::Void);
                    }
                }
                Ty::Unknown
            }
            Expr::StaticAccess {
                type_name, member, ..
            } => {
                let argument_types: Vec<_> = args.iter().map(|arg| self.infer(arg)).collect();
                let Some(method) = self.static_method(type_name, member) else {
                    return Ty::Unknown; // E0307 ownership checks belong to elaboration.
                };
                self.check_args(method, args, &argument_types);
                method
                    .return_type
                    .as_ref()
                    .map(|ty| self.type_of(ty))
                    .unwrap_or(Ty::Void)
            }
            // Unknown callee kinds still have expression children.
            _ => {
                self.infer(callee);
                for arg in args {
                    self.infer(arg);
                }
                Ty::Unknown
            }
        }
    }

    /// Check call arguments against the composed signature's typed
    /// parameters.
    fn check_args(&mut self, method: &MethodDecl, args: &[Expr], kinds: &[Ty]) {
        for (position, arg) in args.iter().enumerate() {
            let Some(param) = method.params.get(position) else {
                break;
            };
            let Some(param_ty) = &param.ty else {
                continue;
            };
            let target = self.type_of(param_ty);
            self.check_assignable(&kinds[position], &target, arg.span(), "argument");
        }
    }

    fn infer_binary(
        &mut self,
        op: crate::ast::BinaryOp,
        lhs: &Expr,
        rhs: &Expr,
        span: crate::ast::Span,
    ) -> Ty {
        use crate::ast::BinaryOp;
        let lhs_ty = self.infer(lhs);
        let rhs_ty = self.infer(rhs);
        match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
                for (side, operand_ty) in [("left", &lhs_ty), ("right", &rhs_ty)] {
                    if has_opinion(operand_ty) && !numeric(operand_ty) {
                        self.diagnostics.push(self.error(
                            "E0223",
                            format!(
                                "the {side} operand of the arithmetic operator is '{}' — a number is required",
                                operand_ty.name()
                            ),
                            span,
                        ));
                    }
                }
                if lhs_ty == Ty::Unknown || rhs_ty == Ty::Unknown {
                    Ty::Unknown
                } else if matches!(lhs_ty, Ty::Real) || matches!(rhs_ty, Ty::Real) {
                    Ty::Real
                } else {
                    Ty::Int
                }
            }
            BinaryOp::Eq
            | BinaryOp::NotEq
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge => {
                self.check_comparison(&lhs_ty, &rhs_ty, span);
                Ty::Bool
            }
            BinaryOp::LogicalAnd | BinaryOp::LogicalOr => {
                for operand_ty in [&lhs_ty, &rhs_ty] {
                    if has_opinion(operand_ty)
                        && !matches!(operand_ty, Ty::Bool | Ty::Int | Ty::Enum(_))
                    {
                        self.diagnostics.push(self.error(
                            "E0223",
                            format!(
                                "the logical operator's operand is '{}' — a boolean is required",
                                operand_ty.name()
                            ),
                            span,
                        ));
                    }
                }
                Ty::Bool
            }
            BinaryOp::BitAnd
            | BinaryOp::BitOr
            | BinaryOp::BitXor
            | BinaryOp::ShiftLeft
            | BinaryOp::ShiftRight => {
                for operand_ty in [&lhs_ty, &rhs_ty] {
                    if has_opinion(operand_ty) && !numeric(operand_ty) {
                        self.diagnostics.push(self.error(
                            "E0223",
                            format!(
                                "the bitwise operator's operand is '{}' — a number is required",
                                operand_ty.name()
                            ),
                            span,
                        ));
                    }
                }
                if lhs_ty == Ty::Unknown || rhs_ty == Ty::Unknown {
                    Ty::Unknown
                } else if matches!(lhs_ty, Ty::Real) || matches!(rhs_ty, Ty::Real) {
                    Ty::Real
                } else {
                    Ty::Int
                }
            }
            // `===`/`!==` accept any kinds by design; `in`/`!in`
            // membership and the string-match operators stay unchecked
            // (v1).
            _ => Ty::Bool,
        }
    }

    /// A comparison of definitely incompatible kinds.
    fn check_comparison(&mut self, lhs: &Ty, rhs: &Ty, span: crate::ast::Span) {
        if !has_opinion(lhs) || !has_opinion(rhs) {
            return;
        }
        if compatible(lhs, rhs, self.types) {
            return;
        }
        // NULL compares against object kinds by design.
        if matches!(lhs, Ty::Void) || matches!(rhs, Ty::Void) {
            return;
        }
        self.diagnostics.push(self.error(
            "E0223",
            format!(
                "the comparison compares '{}' with '{}'",
                lhs.name(),
                rhs.name()
            ),
            span,
        ));
    }

    fn infer_unary(&mut self, op: UnaryOp, expr: &Expr, span: crate::ast::Span) -> Ty {
        let inner = self.infer(expr);
        match op {
            UnaryOp::Not => {
                if has_opinion(&inner) && !matches!(inner, Ty::Bool | Ty::Int | Ty::Enum(_)) {
                    self.diagnostics.push(self.error(
                        "E0223",
                        format!(
                            "'not' applies to '{}' — a boolean is required",
                            inner.name()
                        ),
                        span,
                    ));
                }
                Ty::Bool
            }
            UnaryOp::Negate | UnaryOp::Plus => {
                if has_opinion(&inner) && !numeric(&inner) {
                    self.diagnostics.push(self.error(
                        "E0223",
                        format!(
                            "the sign operator applies to '{}' — a number is required",
                            inner.name()
                        ),
                        span,
                    ));
                }
                if inner == Ty::Unknown {
                    Ty::Unknown
                } else if matches!(inner, Ty::Real) {
                    Ty::Real
                } else {
                    Ty::Int
                }
            }
            UnaryOp::BitNot => {
                if has_opinion(&inner) && !numeric(&inner) {
                    self.diagnostics.push(self.error(
                        "E0223",
                        format!("'~' applies to '{}' — a number is required", inner.name()),
                        span,
                    ));
                }
                if inner == Ty::Unknown {
                    Ty::Unknown
                } else {
                    Ty::Int
                }
            }
        }
    }
}

fn type_name_of(ty: &Ty) -> &str {
    match ty {
        Ty::Object(name) => name,
        _ => "",
    }
}

/// Registered statics are owned by source declaration, not by each
/// inherited/when-view copy. Match provenance even if elaboration rewrote a
/// type expression in the copied member.
fn registered_static_member<'a, T>(
    elaborated: &'a Elaborated,
    registry: &BTreeMap<String, Vec<T>>,
    mut owner: &'a str,
    matches: impl Fn(&T) -> bool,
) -> bool {
    for _ in 0..64 {
        if registry
            .get(owner)
            .is_some_and(|entries| entries.iter().any(&matches))
        {
            return true;
        }
        let Some(base) = elaborated
            .types
            .iter()
            .find(|ty| ty.name == owner)
            .and_then(|ty| ty.base.as_deref())
        else {
            return false;
        };
        owner = base;
    }
    false
}

/// One field's init value against its declared type (6.9.1/6.9.4).
fn check_field_init(checker: &mut TypeChecker, field: &crate::ast::Field) {
    let Some(init) = &field.init else {
        return;
    };
    let value = checker.infer(init);
    let target = checker.type_of(&field.ty);
    checker.check_assignable(&value, &target, init.span(), "field init value");
}

/// An extension can omit result/parameter annotations supplied by its
/// declaration. Explicit annotations retain their own conservative kinds.
fn check_method<'a>(
    checker: &mut TypeChecker<'a>,
    method: &'a MethodDecl,
    prototype: &'a MethodDecl,
) {
    checker.return_ty = method
        .return_type
        .as_ref()
        .or(prototype.return_type.as_ref());
    checker.push_scope();
    for (index, parameter) in method.params.iter().enumerate() {
        let ty = parameter
            .ty
            .as_ref()
            .or_else(|| {
                prototype
                    .params
                    .get(index)
                    .and_then(|parameter| parameter.ty.as_ref())
            })
            .map(|ty| checker.type_of(ty))
            .unwrap_or(Ty::Unknown);
        checker.bind_local(&parameter.name, ty);
    }
    checker.check_body(&method.body);
    checker.pop_scope();
    checker.return_ty = None;
}

/// Check effective Boolean options with the same binding frames as the
/// sampler (14.2-14.5). Source names are locals only in effect predicates;
/// `when` reads the owner, and instance visibility also binds `inst`.
fn check_coverage_options<'a>(
    checker: &mut TypeChecker<'a>,
    owner: &'a ElaboratedType,
    kinds: &BTreeMap<String, BTreeMap<String, Ty>>,
) {
    let mut seen = std::collections::BTreeSet::new();
    for group in &owner.covers {
        if !seen.insert(&group.name) {
            continue;
        }
        let Ok(options) = crate::coverage::group_options(owner, &group.name) else {
            continue; // Structural errors already belong to elaboration.
        };
        checker.check_coverage_boolean(options.when.as_ref(), "when");
        checker.push_scope();
        if let Some(scope) = &options.unit_scope {
            checker.bind_local("inst", Ty::Object(scope.clone()));
        }
        checker.check_coverage_boolean(options.instance_no_collect.as_ref(), "instance_no_collect");
        checker.pop_scope();
        for item in owner
            .covers
            .iter()
            .filter(|decl| decl.name == group.name)
            .flat_map(|decl| &decl.items)
        {
            let Ok(item_options) = crate::coverage::item_options(item) else {
                continue;
            };
            checker.check_coverage_boolean(item_options.when.as_ref(), "when");
            checker.push_scope();
            if let Some(scope) = &options.unit_scope {
                checker.bind_local("inst", Ty::Object(scope.clone()));
            }
            checker.check_coverage_boolean(
                item_options.instance_no_collect.as_ref(),
                "instance_no_collect",
            );
            checker.pop_scope();

            // Ignore/illegal frames have no owner receiver. Basic item
            // aliases shadow owner fields; derived inputs bind only their
            // participating basic sources, with prev_ snapshots for a
            // transition. Composite-valued inputs remain unknown.
            checker.me_type = None;
            checker.push_scope();
            for (name, _) in &item.names {
                let kind = kinds
                    .get(&group.name)
                    .and_then(|group| group.get(name))
                    .cloned()
                    .unwrap_or(Ty::Unknown);
                checker.bind_local(name, kind.clone());
                if item.kind == crate::ast::CoverItemKind::Transition {
                    checker.bind_local(&format!("prev_{name}"), kind);
                }
            }
            // The evaluator inserts the implicit unit after source
            // bindings, even if a basic item itself was named inst.
            if let Some(scope) = &options.unit_scope {
                checker.bind_local("inst", Ty::Object(scope.clone()));
            }
            for (name, expr) in [
                ("ignore", &item_options.ignore),
                ("illegal", &item_options.illegal),
            ] {
                checker.check_coverage_boolean(expr.as_ref(), name);
            }
            for (name, expr) in [
                ("instance_ignore", &item_options.instance_ignore),
                ("instance_illegal", &item_options.instance_illegal),
            ] {
                checker.check_coverage_boolean(expr.as_ref(), name);
            }
            checker.pop_scope();
            checker.me_type = Some(&owner.name);
        }
    }
}

/// The type-check pass over every composed type's members — method
/// bodies, field init values, `keep` constraints, `on` bodies and
/// coverage items and effective Boolean options (14.2-14.5). The
/// `when` blocks' members are checked through their synthesized
/// when-view types (which carry the block's fields as ordinary fields,
/// so sibling access resolves). The diagnostics join the elaboration
/// diagnostics (the CLI prints both).
pub fn check_types(elaborated: &Elaborated) -> Vec<Diagnostic> {
    let mut checker = TypeChecker::new(elaborated);
    for t in &elaborated.types {
        checker.me_type = Some(&t.name);
        checker.return_ty = None;
        for field in &t.fields {
            if field.is_static {
                // The registry owns these entries, including inherited copies.
                if registered_static_member(
                    elaborated,
                    &elaborated.static_fields,
                    &t.name,
                    |entry| entry.name == field.name && entry.name_span == field.name_span,
                ) {
                    continue;
                }
                checker.me_type = None; // Unregistered/template static context stays conservative.
                check_field_init(&mut checker, field);
                checker.me_type = Some(&t.name);
            } else {
                check_field_init(&mut checker, field);
            }
        }
        for constraint in &t.constraints {
            checker.infer(&constraint.expr);
        }
        let mut coverage_kinds = BTreeMap::<String, BTreeMap<String, Ty>>::new();
        for group in &t.covers {
            for item in &group.items {
                if item.kind != crate::ast::CoverItemKind::Item {
                    continue;
                }
                // Like the sampler, resolve an item without an explicit
                // expression to its owner's field. Item names do not
                // introduce locals for other sampled expressions.
                let (value, span) = if let Some(expr) = &item.expr {
                    (checker.infer(expr), expr.span())
                } else if let Some((name, span)) = item.names.first() {
                    (checker.me_field(name).unwrap_or(Ty::Unknown), *span)
                } else {
                    continue;
                };
                if let Some(ty) = &item.ty {
                    let target = checker.type_of(ty);
                    checker.check_assignable(&value, &target, span, "coverage item");
                }
                if let Some((name, _)) = item.names.first() {
                    let kind = item
                        .ty
                        .as_ref()
                        .map(|ty| checker.type_of(ty))
                        .unwrap_or(value);
                    coverage_kinds
                        .entry(group.name.clone())
                        .or_default()
                        .insert(name.clone(), kind);
                }
            }
        }
        check_coverage_options(&mut checker, t, &coverage_kinds);
        for on in &t.ons {
            if on.is_static || on.type_qualified.is_some() {
                if registered_static_member(elaborated, &elaborated.static_ons, &t.name, |entry| {
                    entry.path == on.path
                        && entry.span == on.span
                        && entry.type_qualified == on.type_qualified
                }) {
                    continue;
                }
                checker.me_type = None;
                checker.check_body(&on.body);
                checker.me_type = Some(&t.name);
            } else {
                checker.check_body(&on.body);
            }
        }
        for rule in &t.expects {
            checker.check_temporal(&rule.te);
            checker.check_body(&rule.else_body);
        }
        for method in &t.methods {
            if method.is_static {
                if registered_static_member(
                    elaborated,
                    &elaborated.static_methods,
                    &t.name,
                    |entry| entry.name == method.name && entry.span == method.span,
                ) {
                    continue;
                }
                checker.me_type = None;
                check_method(&mut checker, method, method);
                checker.me_type = Some(&t.name);
            } else {
                check_method(&mut checker, method, method);
            }
        }
    }
    checker.me_type = None;
    // The runtime initializes the static store with no receiver or bare-name
    // static context. Qualified references are still available.
    for fields in elaborated.static_fields.values() {
        for field in fields {
            check_field_init(&mut checker, field);
        }
    }
    for (owner, methods) in &elaborated.static_methods {
        checker.static_context = Some(owner);
        for method in methods {
            let prototype = checker.static_method(owner, &method.name).unwrap_or(method);
            check_method(&mut checker, method, prototype);
        }
    }
    for (owner, ons) in &elaborated.static_ons {
        checker.static_context = Some(owner);
        for on in ons {
            checker.check_body(&on.body);
        }
    }
    checker.static_context = None;
    checker.diagnostics
}
