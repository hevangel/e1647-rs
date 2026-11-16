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
//!   (string against a number, an object in arithmetic — E0223),
//!   including invalid list-index and membership operand kinds.
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
//! their declaration. Known declaration, assignment, field, parameter and
//! return contexts type untyped evaluate/new bodies; calls and expression
//! bodies establish their own scoped hints. Materialized templates retain
//! object kinds and concatenation flattens list operands. Cast results,
//! unresolved contexts/statics remain conservative. Retained type-position
//! widths, keys and value/typeof arguments are traversed in their scopes;
//! definite non-integer widths report E0223. Expressions already eliminated
//! by elaboration are not reconstructed. Known list indexes require
//! integer/enum kinds; membership
//! validates known containers and scalar/list element compatibility (4.11.6).
//! Explicit is-a bindings retain kinds on proven Boolean guard paths,
//! including lazy operands, branches and loops. Ambiguous joins/rebindings
//! lose their kinds; arbitrary flow-sensitive narrowing is not performed.
//! The built-in set kind retains no scalar-domain information; full
//! set/numeric domain rules still need further work.
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
use std::collections::{BTreeMap, BTreeSet};

/// Explicit is-a bindings guaranteed on a Boolean outcome. Possible writes
/// are retained separately: a later conditional binding can replace an earlier
/// one even when its result does not guarantee a particular kind.
#[derive(Default)]
struct GuardFacts {
    yes: BTreeMap<String, Ty>,
    no: BTreeMap<String, Ty>,
    writes: BTreeSet<String>,
}

fn after_guard(
    earlier: &BTreeMap<String, Ty>,
    later: &BTreeMap<String, Ty>,
    writes: &BTreeSet<String>,
) -> BTreeMap<String, Ty> {
    earlier
        .iter()
        .filter(|(name, _)| !writes.contains(*name))
        .chain(later.iter())
        .map(|(name, kind)| (name.clone(), kind.clone()))
        .collect()
}

fn common_bindings(
    left: &BTreeMap<String, Ty>,
    right: &BTreeMap<String, Ty>,
) -> BTreeMap<String, Ty> {
    left.iter()
        .filter(|(name, kind)| right.get(*name) == Some(*kind))
        .map(|(name, kind)| (name.clone(), kind.clone()))
        .collect()
}

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
    /// The built-in set representation; its scalar domain is not tracked.
    Set,
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
            Ty::Set => "set".to_owned(),
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
    /// Scoped contextual result kind for evaluate/new. Calls and expression
    /// bodies establish their own contexts instead of inheriting this hint.
    expected_kind: Option<Ty>,
    types: &'a [ElaboratedType],
    /// Enumerand → its enumeral type name (composed `type` kinds).
    enumeral_table: BTreeMap<String, String>,
    /// The scoped local variables; the last scope is innermost.
    locals: Vec<BTreeMap<String, Ty>>,
    /// Append-only during traversal so nested expressions can report all
    /// possible binding writes, including repeated writes of the same name.
    binding_writes: Vec<String>,
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
            expected_kind: None,
            types,
            enumeral_table,
            locals: vec![BTreeMap::new()],
            binding_writes: Vec::new(),
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

    fn invalidate_binding(&mut self, name: &str) {
        // Runtime bindings may replace a frame local. Outside a guaranteed
        // guard path neither its old kind nor its matched kind is certain.
        if let Some(scope) = self
            .locals
            .iter_mut()
            .rev()
            .find(|scope| scope.contains_key(name))
        {
            scope.insert(name.to_owned(), Ty::Unknown);
        } else {
            self.bind_local(name, Ty::Unknown);
        }
    }

    fn with_bindings<R>(
        &mut self,
        bindings: &BTreeMap<String, Ty>,
        check: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let start = self.binding_writes.len();
        self.push_scope();
        for (name, kind) in bindings {
            self.bind_local(name, kind.clone());
        }
        let result = check(self);
        self.pop_scope();
        // Writes observed under the temporary proof must also invalidate the
        // surrounding frame when the proof's scope is removed.
        for index in start..self.binding_writes.len() {
            let name = self.binding_writes[index].clone();
            self.invalidate_binding(&name);
        }
        result
    }

    /// Infer each operand once and carry explicit binding proofs through the
    /// runtime's lazy Boolean paths (4.11 and 5.4.4.4). No operand narrowing or
    /// arbitrary Boolean algebra is inferred from an unknown expression.
    fn infer_guard(&mut self, expr: &Expr) -> (Ty, GuardFacts) {
        use crate::ast::BinaryOp;
        match expr {
            Expr::Paren { expr, .. } => self.infer_guard(expr),
            Expr::Unary {
                op: UnaryOp::Not,
                expr,
                span,
            } => {
                let (kind, facts) = self.infer_guard(expr);
                self.check_not_kind(&kind, *span);
                (
                    Ty::Bool,
                    GuardFacts {
                        yes: facts.no,
                        no: facts.yes,
                        writes: facts.writes,
                    },
                )
            }
            Expr::Binary {
                op: op @ (BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::Imply),
                lhs,
                rhs,
                span,
            } => {
                let (left_kind, left) = self.infer_guard(lhs);
                let entry = if *op == BinaryOp::LogicalOr {
                    &left.no
                } else {
                    &left.yes
                };
                let (right_kind, right) = self.with_bindings(entry, |this| this.infer_guard(rhs));
                if *op != BinaryOp::Imply {
                    for kind in [&left_kind, &right_kind] {
                        self.check_logical_kind(kind, *span);
                    }
                }
                let rhs_yes = after_guard(entry, &right.yes, &right.writes);
                let rhs_no = after_guard(entry, &right.no, &right.writes);
                let (yes, no) = match op {
                    BinaryOp::LogicalAnd => (rhs_yes, common_bindings(&left.no, &rhs_no)),
                    BinaryOp::LogicalOr => (common_bindings(&left.yes, &rhs_yes), rhs_no),
                    _ => (common_bindings(&left.no, &rhs_yes), rhs_no),
                };
                let writes = left.writes.union(&right.writes).cloned().collect();
                (Ty::Bool, GuardFacts { yes, no, writes })
            }
            _ => {
                let start = self.binding_writes.len();
                let kind = self.infer(expr);
                let writes = self.binding_writes[start..].iter().cloned().collect();
                let mut facts = GuardFacts {
                    writes,
                    ..GuardFacts::default()
                };
                if let Expr::IsA {
                    ty,
                    binding: Some(name),
                    negated: false,
                    ..
                } = expr
                {
                    facts.yes.insert(name.clone(), self.type_of(ty));
                }
                (kind, facts)
            }
        }
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

    /// Walk retained type-position expressions without letting the surrounding
    /// value's expected kind or compile-time bindings leak in/out. Named aliases
    /// are checked at their declaration rather than expanded here.
    fn check_type_expr(&mut self, ty: &TypeExpr) {
        if matches!(
            ty,
            TypeExpr::Named { .. }
                | TypeExpr::Formal { .. }
                | TypeExpr::InlineEnum { .. }
                | TypeExpr::Scalar { width: None, .. }
        ) {
            return;
        }
        let expected = self.expected_kind.take();
        let locals = self.locals.clone();
        let writes = self.binding_writes.len();
        self.check_type_expr_inner(ty, 0);
        self.binding_writes.truncate(writes);
        self.locals = locals;
        self.expected_kind = expected;
    }

    fn check_type_expr_inner(&mut self, ty: &TypeExpr, depth: usize) {
        if depth >= 64 {
            return;
        }
        match ty {
            TypeExpr::Scalar { width, .. } => {
                if let Some(width) = width {
                    self.check_width(width);
                }
            }
            TypeExpr::FormalWidth { width, .. } => self.check_width(width),
            TypeExpr::List { element, key, .. } => {
                self.check_type_expr_inner(element, depth + 1);
                if let Some(key) = key {
                    self.push_scope();
                    self.bind_local("it", self.type_of(element));
                    self.infer_with_hint(None, key);
                    self.pop_scope();
                }
            }
            TypeExpr::WhenQualified { base, .. } => self.check_type_expr_inner(base, depth + 1),
            TypeExpr::TemplateRef { args, .. } => {
                for arg in args {
                    self.check_type_expr_inner(arg, depth + 1);
                }
            }
            TypeExpr::Value { expr, .. }
            | TypeExpr::TypeOf { expr, .. }
            | TypeExpr::TypeOfItem { expr, .. } => {
                self.infer_with_hint(None, expr);
            }
            TypeExpr::Named { .. } | TypeExpr::Formal { .. } | TypeExpr::InlineEnum { .. } => {}
        }
    }

    fn check_width(&mut self, width: &Expr) {
        let kind = self.infer_with_hint(Some(Ty::Int), width);
        if has_opinion(&kind) && !matches!(kind, Ty::Int | Ty::Enum(_)) {
            self.diagnostics.push(self.error(
                "E0223",
                format!(
                    "scalar width has kind '{}' — an integer is required",
                    kind.name()
                ),
                width.span(),
            ));
        }
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
                if name == "set" {
                    return Ty::Set;
                }
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
            TypeExpr::TemplateRef { name, args, .. } => {
                let name = crate::ast::template_instance_name(name, args);
                match self.find_type(&name) {
                    Some(ty) if ty.kind != DeclarationKind::NumericType => Ty::Object(name),
                    _ => Ty::Unknown,
                }
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
                if let Some(ty) = ty {
                    self.check_type_expr(ty);
                }
                let declared = ty.as_ref().map(|ty| self.type_of(ty));
                let inferred = if let Some(init) = init {
                    let value = self.infer_with_hint(declared.clone(), init);
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
                // Resolve the target once while preserving value-before-target
                // diagnostic ordering. Static inference has no runtime effects.
                let before = self.diagnostics.len();
                let target_ty = self.infer(target);
                let target_errors = self.diagnostics.split_off(before);
                let value_ty = self.infer_with_hint(Some(target_ty.clone()), value);
                self.diagnostics.extend(target_errors);
                self.check_assignable(&value_ty, &target_ty, value.span(), "assignment");
            }
            Stmt::ExprStmt { expr, .. } => {
                self.infer(expr);
            }
            Stmt::Return { value, .. } => {
                let Some(value) = value else {
                    return;
                };
                let hint = self.return_ty.map(|ty| self.type_of(ty));
                let value_ty = self.infer_with_hint(hint, value);
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
                let (_, facts) = self.infer_guard(cond);
                self.with_bindings(&facts.yes, |this| this.check_body(then_body));
                if let Some(else_body) = else_body {
                    self.with_bindings(&facts.no, |this| this.check_body(else_body));
                }
            }
            Stmt::While { cond, body, .. } => {
                let (_, facts) = self.infer_guard(cond);
                self.with_bindings(&facts.yes, |this| this.check_body(body));
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
                let (_, facts) = self.infer_guard(condition);
                self.with_bindings(&facts.no, |this| this.check_body(else_body));
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

    fn infer_with_hint(&mut self, hint: Option<Ty>, expr: &Expr) -> Ty {
        let saved = std::mem::replace(&mut self.expected_kind, hint);
        let result = self.infer(expr);
        self.expected_kind = saved;
        result
    }

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
                let base_ty = self.infer_with_hint(None, base);
                let index_ty = self.infer_with_hint(Some(Ty::Int), index);
                if let Ty::List(element) = base_ty {
                    if has_opinion(&index_ty) && !matches!(index_ty, Ty::Int | Ty::Enum(_)) {
                        self.diagnostics.push(self.error(
                            "E0223",
                            format!(
                                "list index has kind '{}' — an integer is required",
                                index_ty.name()
                            ),
                            index.span(),
                        ));
                    }
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
            Expr::Binary {
                op:
                    crate::ast::BinaryOp::LogicalAnd
                    | crate::ast::BinaryOp::LogicalOr
                    | crate::ast::BinaryOp::Imply,
                ..
            } => self.infer_guard(expr).0,
            Expr::Binary { op, lhs, rhs, span } => self.infer_binary(*op, lhs, rhs, *span),
            Expr::Unary { op, expr, span } => self.infer_unary(*op, expr, *span),
            Expr::New { ty, inits, .. } => {
                if let Some(ty) = ty {
                    self.check_type_expr(ty);
                }
                let candidate = ty
                    .as_ref()
                    .map(|ty| self.type_of(ty))
                    .or_else(|| self.expected_kind.clone());
                let candidate = match candidate {
                    Some(Ty::List(element)) if ty.is_none() => Some(*element),
                    other => other,
                };
                let result = match candidate {
                    Some(kind @ (Ty::Object(_) | Ty::Sync | Ty::Iterator)) => kind,
                    _ => Ty::Unknown,
                };
                // The runtime's with block has a fresh frame and the new
                // object as me. Caller locals and return types do not apply.
                let old_me = self.me_type;
                let old_static = self.static_context.take();
                let old_expected = self.expected_kind.take();
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
                self.expected_kind = old_expected;
                result
            }
            Expr::ListConcat { items, .. } => {
                let mut element = None;
                for item in items {
                    let kind = match self.infer(item) {
                        Ty::List(element) => *element, // Concatenation flattens list operands.
                        kind => kind,
                    };
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
                let (_, facts) = self.infer_guard(cond);
                let left = self.with_bindings(&facts.yes, |this| this.infer(then_expr));
                let right = self.with_bindings(&facts.no, |this| this.infer(else_expr));
                shared_kind(left, right)
            }
            Expr::InSet { element, set, span } => {
                self.check_membership(element, set, *span);
                Ty::Bool
            }
            Expr::IsA {
                expr,
                ty,
                binding,
                negated,
                ..
            } => {
                self.infer(expr);
                self.check_type_expr(ty);
                if !*negated && let Some(name) = binding {
                    self.binding_writes.push(name.clone());
                    self.invalidate_binding(name);
                }
                Ty::Bool
            }
            Expr::CastAs { expr, ty, .. } => {
                self.infer(expr);
                self.check_type_expr(ty);
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
                            crate::ast::TableElem::TypeSet { ty, .. } => self.check_type_expr(ty),
                        }
                    }
                }
                Ty::Bool
            }
            Expr::SetLiteral { items, .. } => {
                self.check_set(items);
                Ty::Set
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
                if let Some(ty) = ty {
                    self.check_type_expr(ty);
                }
                let result = ty
                    .as_ref()
                    .map(|ty| self.type_of(ty))
                    .or_else(|| self.expected_kind.clone())
                    .unwrap_or(Ty::Unknown);
                let saved = self.expected_kind.take();
                self.push_scope();
                self.bind_local(var.as_deref().unwrap_or("value"), result.clone());
                self.check_body(body);
                self.pop_scope();
                self.expected_kind = saved;
                result
            }
            Expr::StaticAccess {
                type_name, member, ..
            } => self.static_field(type_name, member).unwrap_or(Ty::Unknown),
            // Leaves without ordinary expression children.
            Expr::Formal { .. } | Expr::HdlPath { .. } | Expr::AnnotationTypeRef { .. } => {
                Ty::Unknown
            }
            Expr::SetOfValues { ty, .. } => {
                self.check_type_expr(ty);
                Ty::Set
            }
        }
    }

    /// Membership accepts a scalar or a contained list (4.11.6). Do not
    /// confuse list containment with comparing a list against one scalar.
    /// The set representation erases its scalar domain, so domain-specific
    /// enum/bool/numeric compatibility remains conservative.
    fn check_membership(
        &mut self,
        element: &Expr,
        set: &crate::ast::InSetExpr,
        span: crate::ast::Span,
    ) {
        let kind = self.infer_with_hint(None, element);
        let item = match &kind {
            Ty::List(item) => item.as_ref(),
            kind => kind,
        };
        match set {
            crate::ast::InSetExpr::Items(items) => {
                for value in items {
                    let other = self.infer_with_hint(None, value);
                    self.check_comparison(item, &other, value.span());
                }
            }
            crate::ast::InSetExpr::Range(low, high) => {
                let low_kind = self.infer_with_hint(None, low);
                let high_kind = self.infer_with_hint(None, high);
                for (kind, location) in [
                    (item, element.span()),
                    (&low_kind, low.span()),
                    (&high_kind, high.span()),
                ] {
                    self.check_set_scalar(kind, location);
                }
            }
            crate::ast::InSetExpr::Expr(container) => {
                let container_kind = self.infer_with_hint(None, container);
                match container_kind {
                    Ty::List(other) => self.check_comparison(item, &other, span),
                    Ty::Set => self.check_set_scalar(item, element.span()),
                    Ty::Unknown => {}
                    kind => self.diagnostics.push(self.error(
                        "E0223",
                        format!("membership requires a list or set, got '{}'", kind.name()),
                        container.span(),
                    )),
                }
            }
        }
    }

    fn check_set_scalar(&mut self, kind: &Ty, span: crate::ast::Span) {
        // Real/numeric conversion and enumeral domains need finer types;
        // reject only definitely non-scalar families here (5.2.12).
        if has_opinion(kind) && !matches!(kind, Ty::Int | Ty::Real | Ty::Enum(_) | Ty::Bool) {
            self.diagnostics.push(self.error(
                "E0223",
                format!("set membership operand has kind '{}' — a numeric, boolean or enumeral value is required", kind.name()),
                span,
            ));
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

    fn infer_call(&mut self, callee: &Expr, args: &[Expr], span: crate::ast::Span) -> Ty {
        let saved = self.expected_kind.take();
        let result = self.infer_call_inner(callee, args, span);
        self.expected_kind = saved;
        result
    }

    fn infer_arguments(&mut self, args: &[Expr], method: Option<&MethodDecl>) -> Vec<Ty> {
        args.iter()
            .enumerate()
            .map(|(index, arg)| {
                let hint = method
                    .and_then(|method| method.params.get(index))
                    .and_then(|parameter| parameter.ty.as_ref())
                    .map(|ty| self.type_of(ty));
                self.infer_with_hint(hint, arg)
            })
            .collect()
    }

    fn infer_call_inner(&mut self, callee: &Expr, args: &[Expr], _span: crate::ast::Span) -> Ty {
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
                    .and_then(|t| {
                        t.methods
                            .iter()
                            .find(|m| m.name == *name && m.modifier.is_none())
                            .or_else(|| t.methods.iter().find(|m| m.name == *name))
                    });
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
                let argument_types = self.infer_arguments(args, Some(method));
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
                        let predicate =
                            matches!(name.as_str(), "has" | "count" | "all" | "all_indices");
                        let argument_ty = self.infer_with_hint(predicate.then_some(Ty::Bool), arg);
                        if predicate {
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
                let method = self.find_type(type_name_of(&object_ty)).and_then(|ty| {
                    ty.methods
                        .iter()
                        .find(|method| method.name == *name && method.modifier.is_none())
                        .or_else(|| ty.methods.iter().find(|method| method.name == *name))
                });
                let argument_types = self.infer_arguments(args, method);
                match &object_ty {
                    Ty::Object(_) => {
                        if let Some(method) = method {
                            self.check_args(method, args, &argument_types);
                            method
                                .return_type
                                .as_ref()
                                .map(|ty| self.type_of(ty))
                                .unwrap_or(Ty::Void)
                        } else {
                            self.infer_member(&object_ty, name, *member_span)
                        }
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
                let method = self.find_type(type_name_of(&object_ty)).and_then(|ty| {
                    ty.methods
                        .iter()
                        .find(|method| method.name == *field && method.modifier.is_none())
                        .or_else(|| ty.methods.iter().find(|method| method.name == *field))
                });
                let argument_types = self.infer_arguments(args, method);
                if let Some(method) = method {
                    self.check_args(method, args, &argument_types);
                    return method
                        .return_type
                        .as_ref()
                        .map(|ty| self.type_of(ty))
                        .unwrap_or(Ty::Void);
                }
                Ty::Unknown
            }
            Expr::StaticAccess {
                type_name, member, ..
            } => {
                let method = self.static_method(type_name, member);
                let argument_types = self.infer_arguments(args, method);
                let Some(method) = method else {
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
                self.check_not_kind(&inner, span);
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

    fn check_not_kind(&mut self, kind: &Ty, span: crate::ast::Span) {
        if has_opinion(kind) && !matches!(kind, Ty::Bool | Ty::Int | Ty::Enum(_)) {
            self.diagnostics.push(self.error(
                "E0223",
                format!("'not' applies to '{}' — a boolean is required", kind.name()),
                span,
            ));
        }
    }

    fn check_logical_kind(&mut self, kind: &Ty, span: crate::ast::Span) {
        if has_opinion(kind) && !matches!(kind, Ty::Bool | Ty::Int | Ty::Enum(_)) {
            self.diagnostics.push(self.error(
                "E0223",
                format!(
                    "the logical operator's operand is '{}' — a boolean is required",
                    kind.name()
                ),
                span,
            ));
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
    checker.check_type_expr(&field.ty);
    let Some(init) = &field.init else {
        return;
    };
    let target = checker.type_of(&field.ty);
    let value = checker.infer_with_hint(Some(target.clone()), init);
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
    // Bind every parameter before checking dependent annotation expressions;
    // even physically later parameters shadow owner fields.
    for parameter in &method.params {
        if let Some(ty) = &parameter.ty {
            checker.check_type_expr(ty);
        }
    }
    if let Some(ty) = &method.return_type {
        checker.check_type_expr(ty);
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
        if let Some(ty) = &t.underlying {
            checker.check_type_expr(ty);
        }
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
                    checker.check_type_expr(ty);
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
                let prototype = t
                    .methods
                    .iter()
                    .find(|entry| entry.name == method.name && entry.modifier.is_none())
                    .unwrap_or(method);
                check_method(&mut checker, method, prototype);
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
