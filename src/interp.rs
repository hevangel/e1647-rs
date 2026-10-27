//! An interpreter for the executable subset of the parsed language.
//!
//! Entry point: [`run`]. It instantiates the built-in `sys` unit and
//! executes every composed `run()` method on it, in composition order. A
//! file without `sys` (or without `run()` on `sys`) executes to completion
//! with no output — declarations-only programs run and print nothing.
//!
//! Supported at run time: `uint`/`int`/`bit`/`byte`/`nibble`/`bool`/`real`/
//! `string` values (integers are unbounded `i128`; declared widths are not
//! truncated), enumerals, `list of` values with the `size()`/`push()`
//! methods, struct instances, method calls (every composed block with the
//! name runs, in composition order; the last returned value wins), local
//! variables, assignments, `if`/`else`, `while` with `break`/`continue`,
//! `return`, `print`, the built-in `out` routine, and `gen`.
//!
//! `gen` is backed by [`crate::solve`]: with the `solver-pumpkin` feature
//! the type's conjunctive linear `keep` constraints are solved (seeded,
//! deterministic Pumpkin CP solve) and unsupported shapes fall back to the
//! rejection sampler, **not** a general constraint solver: fields are
//! sampled inside their declared bounds, a candidate is accepted only when
//! every top-level `keep` constraint of the enclosing type evaluates to
//! TRUE, and after 1000 failed attempts a diagnostic is reported. Soft
//! constraints and `when` blocks are not enforced. Every run takes an
//! explicit seed and is fully deterministic per backend.
//!
//! ## Diagnostic codes
//!
//! - `E0401` unknown identifier, field or method at run time
//! - `E0402` type mismatch or invalid operand
//! - `E0403` division by zero
//! - `E0404` list index out of range
//! - `E0405` call to an unsupported routine or list method
//! - `E0406` recursion too deep
//! - `E0407` no solution found for the constraints (1000 attempts)

use crate::ast::{
    BinaryOp, DeclarationKind, Expr, MethodDecl, ScalarBase, Stmt, TypeExpr, UnaryOp,
};
use crate::diagnostic::Diagnostic;
use crate::elab::Elaborated;
use crate::lexer::Position;

/// Outcome of one run: the text produced by `out`/`print` and any run-time
/// diagnostics.
#[derive(Debug, Clone, PartialEq)]
pub struct RunResult {
    pub output: String,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone)]
#[allow(clippy::derive_partial_eq_without_eq)] // Real(f64) has no total equality
pub enum Value {
    Int(i128),
    Real(f64),
    Bool(bool),
    Str(String),
    Enum { type_name: String, name: String },
    List(Vec<Value>),
    Object(usize),
    Void,
}

struct ObjectInstance {
    type_name: String,
    /// Declaration order preserved; `gen` and formatting rely on it.
    fields: Vec<(String, Value)>,
}

impl ObjectInstance {
    fn get(&self, name: &str) -> Option<&Value> {
        self.fields
            .iter()
            .find(|(field, _)| field == name)
            .map(|(_, value)| value)
    }

    fn set(&mut self, name: &str, value: Value) -> bool {
        for (field, slot) in &mut self.fields {
            if field == name {
                *slot = value;
                return true;
            }
        }
        false
    }
}

#[derive(Default)]
struct Frame {
    locals: std::collections::HashMap<String, Value>,
    local_types: std::collections::HashMap<String, TypeExpr>,
}

enum Flow {
    Normal,
    Break,
    Continue,
    Return(Value),
}

/// splitmix64: small, seedable, deterministic — no external dependency.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: u128) -> u128 {
        if bound == 0 {
            0
        } else {
            u128::from(self.next_u64()) % bound
        }
    }

    fn unit_f64(&mut self) -> f64 {
        f64::from((self.next_u64() >> 40) as u32) / f64::from(1u32 << 24)
    }
}

struct Interp<'a> {
    types: &'a [crate::elab::ElaboratedType],
    instances: Vec<ObjectInstance>,
    rng: Rng,
    /// The run's seed, handed to the constraint-solver backend for `gen`.
    /// Only read with the `solver-pumpkin` feature.
    #[cfg_attr(not(feature = "solver-pumpkin"), allow(dead_code))]
    seed: u64,
    output: String,
    depth: usize,
}

/// Instantiate the built-in `sys` unit and execute its `run()` methods.
pub fn run(elaborated: &Elaborated, seed: u64) -> RunResult {
    let mut interp = Interp {
        types: elaborated.types.as_slice(),
        instances: Vec::new(),
        rng: Rng::new(seed),
        seed,
        output: String::new(),
        depth: 0,
    };
    let types: &[crate::elab::ElaboratedType] = interp.types;
    let Some(sys) = types
        .iter()
        .find(|t| t.name == "sys" && t.kind == DeclarationKind::Unit)
    else {
        // No `sys`: a declarations-only program runs to completion with no
        // output.
        return RunResult {
            output: interp.output,
            diagnostics: Vec::new(),
        };
    };
    let run_blocks: Vec<&MethodDecl> = sys
        .methods
        .iter()
        .filter(|method| method.name == "run")
        .collect();
    if run_blocks.is_empty() {
        return RunResult {
            output: interp.output,
            diagnostics: Vec::new(),
        };
    }
    let sys_index = interp.instantiate("sys");
    let mut diagnostics = Vec::new();
    for block in run_blocks {
        let mut frame = Frame::default();
        if let Err(diagnostic) = interp.exec_block(&block.body, &mut frame, Some(sys_index)) {
            diagnostics.push(diagnostic);
            break;
        }
    }
    RunResult {
        output: interp.output,
        diagnostics,
    }
}

fn origin() -> Position {
    Position {
        byte: 0,
        line: 1,
        column: 1,
    }
}

fn origin_span() -> crate::ast::Span {
    (origin(), origin())
}

impl<'a> Interp<'a> {
    fn instantiate(&mut self, type_name: &str) -> usize {
        let mut fields = Vec::new();
        let types: &[crate::elab::ElaboratedType] = self.types;
        if let Some(ty) = types.iter().find(|t| t.name == type_name) {
            for field in &ty.fields {
                let default = self.default_for(&field.ty);
                fields.push((field.name.clone(), default));
            }
        }
        self.instances.push(ObjectInstance {
            type_name: type_name.to_owned(),
            fields,
        });
        self.instances.len() - 1
    }

    fn default_for(&mut self, ty: &TypeExpr) -> Value {
        match ty {
            TypeExpr::Scalar { base, .. } => match base {
                ScalarBase::Bool => Value::Bool(false),
                ScalarBase::Real => Value::Real(0.0),
                ScalarBase::String => Value::Str(String::new()),
                _ => Value::Int(0),
            },
            TypeExpr::List { .. } => Value::List(Vec::new()),
            TypeExpr::Named { name, .. } => {
                let types: &[crate::elab::ElaboratedType] = self.types;
                if let Some(t) = types.iter().find(|t| t.name == name.as_str())
                    && t.kind == DeclarationKind::Type
                    && let Some(first) = t.enumerals.first()
                {
                    return Value::Enum {
                        type_name: name.clone(),
                        name: first.name.clone(),
                    };
                }
                if self.depth > 100 {
                    return Value::Void;
                }
                self.depth += 1;
                let index = self.instantiate(name);
                self.depth -= 1;
                Value::Object(index)
            }
        }
    }

    fn exec_block(
        &mut self,
        statements: &[Stmt],
        frame: &mut Frame,
        me: Option<usize>,
    ) -> Result<Flow, Diagnostic> {
        for statement in statements {
            match self.exec_statement(statement, frame, me)? {
                Flow::Normal => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Normal)
    }

    fn exec_statement(
        &mut self,
        statement: &Stmt,
        frame: &mut Frame,
        me: Option<usize>,
    ) -> Result<Flow, Diagnostic> {
        match statement {
            Stmt::VarDecl { name, ty, init, .. } => {
                let value = match init {
                    Some(expr) => self.eval(expr, frame, me)?,
                    None => self.default_for(ty),
                };
                frame.local_types.insert(name.clone(), ty.clone());
                frame.locals.insert(name.clone(), value);
                Ok(Flow::Normal)
            }
            Stmt::Assign { target, value, .. } => {
                let value = self.eval(value, frame, me)?;
                match target {
                    Expr::Index { base, index, span } => {
                        let index_value = self.eval(index, frame, me)?;
                        let Value::Int(i) = index_value else {
                            return Err(self.error(
                                "E0402",
                                "list index must be an integer",
                                *span,
                            ));
                        };
                        let mut list = self.read_slot(base, frame, me)?;
                        let Value::List(items) = &mut list else {
                            return Err(self.error("E0402", "indexed value is not a list", *span));
                        };
                        let len = items.len() as i128;
                        if i < 0 || i >= len {
                            return Err(self.error(
                                "E0404",
                                format!("list index {i} out of range (size {len})"),
                                *span,
                            ));
                        }
                        items[i as usize] = value;
                        self.write_slot(base, list, frame, me, *span)?;
                        Ok(Flow::Normal)
                    }
                    _ => {
                        self.write_slot(target, value, frame, me, statement.span())?;
                        Ok(Flow::Normal)
                    }
                }
            }
            Stmt::ExprStmt { expr, .. } => {
                self.eval(expr, frame, me)?;
                Ok(Flow::Normal)
            }
            Stmt::Return { value, .. } => {
                let value = match value {
                    Some(expr) => self.eval(expr, frame, me)?,
                    None => Value::Void,
                };
                Ok(Flow::Return(value))
            }
            Stmt::If {
                cond,
                then_body,
                else_body,
                ..
            } => {
                let Value::Bool(cond_value) = self.eval(cond, frame, me)? else {
                    let span = statement.span();
                    return Err(self.error("E0402", "condition is not a boolean", span));
                };
                if cond_value {
                    self.exec_block(then_body, frame, me)
                } else if let Some(else_body) = else_body {
                    self.exec_block(else_body, frame, me)
                } else {
                    Ok(Flow::Normal)
                }
            }
            Stmt::While { cond, body, .. } => {
                loop {
                    let Value::Bool(cond_value) = self.eval(cond, frame, me)? else {
                        let span = statement.span();
                        return Err(self.error("E0402", "condition is not a boolean", span));
                    };
                    if !cond_value {
                        break;
                    }
                    match self.exec_block(body, frame, me)? {
                        Flow::Break => break,
                        Flow::Continue | Flow::Normal => {}
                        Flow::Return(value) => return Ok(Flow::Return(value)),
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::Break { .. } => Ok(Flow::Break),
            Stmt::Continue { .. } => Ok(Flow::Continue),
            Stmt::Print { exprs, .. } => {
                let mut parts = Vec::new();
                for expr in exprs {
                    let value = self.eval(expr, frame, me)?;
                    let text = match expr {
                        Expr::Ident { name, .. } => {
                            format!("{name} = {}", self.format_value(&value))
                        }
                        _ => self.format_value(&value),
                    };
                    parts.push(text);
                }
                self.output.push_str(&parts.join(" "));
                self.output.push('\n');
                Ok(Flow::Normal)
            }
            Stmt::Gen { expr, span } => self.exec_gen(expr, frame, me, *span),
        }
    }

    fn exec_gen(
        &mut self,
        expr: &Expr,
        frame: &mut Frame,
        me: Option<usize>,
        span: crate::ast::Span,
    ) -> Result<Flow, Diagnostic> {
        let Expr::Ident { name, .. } = expr else {
            return Err(self.error("E0402", "gen target must be a variable", span));
        };
        let Some(me_index) = me else {
            return Err(self.error("E0402", "gen requires an enclosing instance", span));
        };
        let slot_is_local = frame.locals.contains_key(name);
        let current = if slot_is_local {
            frame.locals.get(name).cloned()
        } else {
            self.instances[me_index].get(name).cloned()
        };
        let Some(current) = current else {
            return Err(self.error("E0401", format!("unknown variable '{name}' in gen"), span));
        };
        // A struct instance is randomized in place, recursively.
        if let Value::Object(index) = current {
            self.gen_object(index)?;
            return Ok(Flow::Normal);
        }
        // Scalar, list or enumeral target: sample candidates and accept one
        // that satisfies every top-level constraint of the enclosing type.
        let Some(declared_ty) = self.declared_type_of(name, slot_is_local, frame, me_index) else {
            return Err(self.error(
                "E0402",
                format!("cannot determine the type of '{name}'"),
                span,
            ));
        };
        let obj_type_name = self.instances[me_index].type_name.clone();
        let types: &[crate::elab::ElaboratedType] = self.types;
        let constraints: Vec<Expr> = types
            .iter()
            .find(|t| t.name == obj_type_name)
            .map(|t| t.constraints.iter().map(|c| c.expr.clone()).collect())
            .unwrap_or_default();
        // Solver-backed path (feature `solver-pumpkin`).
        #[cfg(feature = "solver-pumpkin")]
        {
            let bounds: std::collections::BTreeMap<String, (i64, i64)> =
                crate::solve::int_bounds(&declared_ty)
                    .map(|b| (name.clone(), b))
                    .into_iter()
                    .collect();
            if !bounds.is_empty()
                && let Ok(problem) = crate::solve::translate(&constraints, &bounds)
            {
                match crate::solve::solve(&problem, self.seed) {
                    crate::solve::SolveOutcome::Solved(mut assignment) => {
                        let value = assignment.remove(0);
                        self.store(
                            slot_is_local,
                            name,
                            Value::Int(value),
                            frame,
                            me_index,
                            span,
                        )?;
                        return Ok(Flow::Normal);
                    }
                    crate::solve::SolveOutcome::Unsat => {
                        return Err(self.error(
                            "E0407",
                            format!("the constraints of '{obj_type_name}' are unsatisfiable"),
                            span,
                        ));
                    }
                    crate::solve::SolveOutcome::Unknown
                    | crate::solve::SolveOutcome::Unsupported => {}
                }
            }
        }
        let mut solved = false;
        for _ in 0..1000 {
            let candidate = self.sample_type(&declared_ty)?;
            self.store(slot_is_local, name, candidate, frame, me_index, span)?;
            let mut pass = true;
            for constraint in &constraints {
                match self.eval(constraint, frame, Some(me_index))? {
                    Value::Bool(true) => {}
                    Value::Bool(false) => {
                        pass = false;
                        break;
                    }
                    _ => {
                        return Err(self.error(
                            "E0402",
                            "constraint did not evaluate to a boolean",
                            span,
                        ));
                    }
                }
            }
            if pass {
                solved = true;
                break;
            }
        }
        if !solved {
            return Err(self.error(
                "E0407",
                format!(
                    "no solution found for the constraints of '{obj_type_name}' after 1000 attempts"
                ),
                span,
            ));
        }
        Ok(Flow::Normal)
    }

    fn declared_type_of(
        &self,
        name: &str,
        slot_is_local: bool,
        frame: &Frame,
        me: usize,
    ) -> Option<TypeExpr> {
        if slot_is_local {
            return frame.local_types.get(name).cloned();
        }
        let obj_type = self.instances[me].type_name.clone();
        let types: &[crate::elab::ElaboratedType] = self.types;
        let ty = types.iter().find(|t| t.name == obj_type)?;
        ty.fields
            .iter()
            .find(|field| field.name == name)
            .map(|field| field.ty.clone())
    }

    fn store(
        &mut self,
        slot_is_local: bool,
        name: &str,
        value: Value,
        frame: &mut Frame,
        me: usize,
        span: crate::ast::Span,
    ) -> Result<(), Diagnostic> {
        if slot_is_local {
            frame.locals.insert(name.to_owned(), value);
            return Ok(());
        }
        if self.instances[me].set(name, value) {
            Ok(())
        } else {
            Err(self.error("E0401", format!("unknown field '{name}'"), span))
        }
    }

    /// Randomize one struct instance in place: every field is sampled and
    /// the type's top-level `keep` constraints must all hold.
    fn gen_object(&mut self, index: usize) -> Result<(), Diagnostic> {
        let type_name = self.instances[index].type_name.clone();
        let types: &[crate::elab::ElaboratedType] = self.types;
        let Some(ty) = types.iter().find(|t| t.name == type_name) else {
            return Err(self.error(
                "E0401",
                format!("unknown type '{type_name}'"),
                origin_span(),
            ));
        };
        let type_span = ty.span;
        let field_types: Vec<(String, TypeExpr)> = ty
            .fields
            .iter()
            .map(|field| (field.name.clone(), field.ty.clone()))
            .collect();
        let constraints: Vec<Expr> = ty
            .constraints
            .iter()
            .map(|constraint| constraint.expr.clone())
            .collect();
        // Solver-backed path (feature `solver-pumpkin`): the integer
        // scalar fields are solved, the remaining fields are sampled.
        #[cfg(feature = "solver-pumpkin")]
        if let Some(assignment) =
            self.try_solve_fields(&field_types, &constraints, &type_name, type_span)?
        {
            for (name, field_ty) in &field_types {
                match assignment.iter().find(|(assigned, _)| assigned == name) {
                    Some((_, value)) => {
                        self.instances[index].set(name, Value::Int(*value));
                    }
                    None => {
                        let value = self.sample_type(field_ty)?;
                        self.instances[index].set(name, value);
                    }
                }
            }
            return Ok(());
        }
        for _ in 0..1000 {
            for (name, field_ty) in &field_types {
                let value = self.sample_type(field_ty)?;
                self.instances[index].set(name, value);
            }
            let mut pass = true;
            for constraint in &constraints {
                match self.eval(constraint, &mut Frame::default(), Some(index))? {
                    Value::Bool(true) => {}
                    Value::Bool(false) => {
                        pass = false;
                        break;
                    }
                    _ => {
                        return Err(self.error(
                            "E0402",
                            "constraint did not evaluate to a boolean",
                            type_span,
                        ));
                    }
                }
            }
            if pass {
                return Ok(());
            }
        }
        Err(self.error(
            "E0407",
            format!("no solution found for the constraints of '{type_name}' after 1000 attempts"),
            type_span,
        ))
    }

    /// Translate the type's `keep` constraints over its integer scalar
    /// fields and solve them with the constraint-solver backend (pumpkin).
    /// `Ok(None)` means the caller must use the rejection sampler:
    /// translation unsupported, backend unavailable, or search unknown.
    #[cfg(feature = "solver-pumpkin")]
    fn try_solve_fields(
        &self,
        field_types: &[(String, TypeExpr)],
        constraints: &[Expr],
        type_name: &str,
        span: crate::ast::Span,
    ) -> Result<Option<Vec<(String, i128)>>, Diagnostic> {
        let bounds: std::collections::BTreeMap<String, (i64, i64)> = field_types
            .iter()
            .filter_map(|(name, ty)| crate::solve::int_bounds(ty).map(|b| (name.clone(), b)))
            .collect();
        if bounds.is_empty() {
            return Ok(None);
        }
        let problem = match crate::solve::translate(constraints, &bounds) {
            Ok(problem) => problem,
            Err(_) => return Ok(None),
        };
        match crate::solve::solve(&problem, self.seed) {
            crate::solve::SolveOutcome::Solved(assignment) => Ok(Some(
                problem
                    .vars
                    .iter()
                    .map(|var| var.name.clone())
                    .zip(assignment)
                    .collect(),
            )),
            crate::solve::SolveOutcome::Unsat => Err(self.error(
                "E0407",
                format!("the constraints of '{type_name}' are unsatisfiable"),
                span,
            )),
            crate::solve::SolveOutcome::Unknown | crate::solve::SolveOutcome::Unsupported => {
                Ok(None)
            }
        }
    }

    /// Sample one value of the declared type within its bounds. Struct
    /// fields are instantiated and randomized recursively (so their own
    /// constraints are enforced); non-literal widths use the default.
    fn sample_type(&mut self, ty: &TypeExpr) -> Result<Value, Diagnostic> {
        match ty {
            TypeExpr::Scalar { base, width, .. } => Ok(match base {
                ScalarBase::Uint => Value::Int(self.below_width(width, 32)),
                ScalarBase::Int => {
                    let bits = width_bits(width, 32);
                    let raw = self.rng.below(1u128 << bits) as i128;
                    Value::Int(raw - ((1i128 << bits) >> 1))
                }
                ScalarBase::Bit => Value::Int(self.rng.below(2) as i128),
                ScalarBase::Byte => Value::Int(self.rng.below(256) as i128),
                ScalarBase::Nibble => Value::Int(self.rng.below(16) as i128),
                ScalarBase::Bool => Value::Bool(self.rng.below(2) == 1),
                ScalarBase::Real => Value::Real(self.rng.unit_f64()),
                ScalarBase::String => Value::Str(String::new()),
                ScalarBase::Time => Value::Int(0),
            }),
            TypeExpr::List { element, .. } => {
                let len = self.rng.below(4) as usize;
                let mut items = Vec::with_capacity(len);
                for _ in 0..len {
                    items.push(self.sample_type(element)?);
                }
                Ok(Value::List(items))
            }
            TypeExpr::Named { name, .. } => {
                let types: &[crate::elab::ElaboratedType] = self.types;
                if let Some(t) = types.iter().find(|t| t.name == name.as_str())
                    && t.kind == DeclarationKind::Type
                {
                    let names: Vec<&str> = t.enumerals.iter().map(|e| e.name.as_str()).collect();
                    if names.is_empty() {
                        return Ok(Value::Void);
                    }
                    let pick = names[self.rng.below(names.len() as u128) as usize];
                    return Ok(Value::Enum {
                        type_name: name.clone(),
                        name: pick.to_owned(),
                    });
                }
                if self.depth > 50 {
                    return Err(self.error("E0406", "recursion too deep", origin_span()));
                }
                self.depth += 1;
                let index = self.instantiate(name);
                let result = self.gen_object(index);
                self.depth -= 1;
                result?;
                Ok(Value::Object(index))
            }
        }
    }

    fn below_width(&mut self, width: &Option<Box<Expr>>, default_bits: u32) -> i128 {
        self.rng.below(1u128 << width_bits(width, default_bits)) as i128
    }

    fn call_method(
        &mut self,
        object_index: usize,
        name: &str,
        args: Vec<Value>,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        let type_name = self.instances[object_index].type_name.clone();
        let types: &[crate::elab::ElaboratedType] = self.types;
        let Some(ty) = types.iter().find(|t| t.name == type_name) else {
            return Err(self.error("E0401", format!("unknown type '{type_name}'"), span));
        };
        let blocks: Vec<&MethodDecl> = ty
            .methods
            .iter()
            .filter(|method| method.name == name)
            .collect();
        if blocks.is_empty() {
            return Err(self.error(
                "E0401",
                format!("method '{name}' not found on '{type_name}'"),
                span,
            ));
        }
        if self.depth > 200 {
            return Err(self.error("E0406", "recursion too deep", span));
        }
        self.depth += 1;
        let mut result = Value::Void;
        for block in blocks {
            if args.len() != block.params.len() {
                return Err(self.error(
                    "E0402",
                    format!(
                        "method '{name}' expects {} argument(s), got {}",
                        block.params.len(),
                        args.len()
                    ),
                    span,
                ));
            }
            let mut frame = Frame::default();
            for (param, arg) in block.params.iter().zip(args.iter()) {
                frame
                    .local_types
                    .insert(param.name.clone(), param.ty.clone());
                frame.locals.insert(param.name.clone(), arg.clone());
            }
            match self.exec_block(&block.body, &mut frame, Some(object_index))? {
                Flow::Return(value) => result = value,
                Flow::Normal | Flow::Break | Flow::Continue => {}
            }
        }
        self.depth -= 1;
        Ok(result)
    }

    fn eval(
        &mut self,
        expr: &Expr,
        frame: &mut Frame,
        me: Option<usize>,
    ) -> Result<Value, Diagnostic> {
        match expr {
            Expr::IntLiteral { text, .. } => {
                parse_int_literal(text).map(Value::Int).ok_or_else(|| {
                    self.error(
                        "E0402",
                        format!("invalid integer literal '{text}'"),
                        expr.span(),
                    )
                })
            }
            Expr::RealLiteral { text, .. } => text
                .replace('_', "")
                .parse::<f64>()
                .map(Value::Real)
                .map_err(|_| {
                    self.error(
                        "E0402",
                        format!("invalid real literal '{text}'"),
                        expr.span(),
                    )
                }),
            Expr::BitLiteral { text, .. } => {
                parse_int_literal(text).map(Value::Int).ok_or_else(|| {
                    self.error(
                        "E0402",
                        format!("invalid based literal '{text}'"),
                        expr.span(),
                    )
                })
            }
            Expr::BoolLiteral { value, .. } => Ok(Value::Bool(*value)),
            Expr::StringLiteral { value, .. } => Ok(Value::Str(unescape(value))),
            Expr::Ident { name, span } => {
                if name == "me" || name == "self" {
                    return match me {
                        Some(index) => Ok(Value::Object(index)),
                        None => Err(self.error("E0402", "no enclosing instance for 'me'", *span)),
                    };
                }
                if let Some(value) = frame.locals.get(name) {
                    return Ok(value.clone());
                }
                if let Some(index) = me
                    && let Some(value) = self.instances[index].get(name)
                {
                    return Ok(value.clone());
                }
                let types: &[crate::elab::ElaboratedType] = self.types;
                for t in types.iter().filter(|t| t.kind == DeclarationKind::Type) {
                    if t.enumerals.iter().any(|e| e.name == *name) {
                        return Ok(Value::Enum {
                            type_name: t.name.clone(),
                            name: name.clone(),
                        });
                    }
                }
                Err(self.error("E0401", format!("unknown identifier '{name}'"), *span))
            }
            Expr::MemberAccess {
                object, name, span, ..
            } => {
                let object_value = self.eval(object, frame, me)?;
                match object_value {
                    Value::Object(index) => {
                        self.instances[index].get(name).cloned().ok_or_else(|| {
                            self.error("E0401", format!("unknown field '{name}'"), *span)
                        })
                    }
                    other => Err(self.error(
                        "E0402",
                        format!(
                            "cannot access member '{name}' of {}",
                            self.value_kind(&other)
                        ),
                        *span,
                    )),
                }
            }
            Expr::Call { callee, args, span } => {
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(self.eval(arg, frame, me)?);
                }
                match callee.as_ref() {
                    Expr::Ident { name, .. } => {
                        if crate::builtins::is_builtin_routine(name) {
                            if name == "out" {
                                let text: String =
                                    values.iter().map(|v| self.format_value(v)).collect();
                                self.output.push_str(&text);
                                return Ok(Value::Void);
                            }
                            return Err(self.error(
                                "E0405",
                                format!("routine '{name}' is not supported"),
                                *span,
                            ));
                        }
                        let Some(index) = me else {
                            return Err(self.error(
                                "E0402",
                                format!("call to '{name}' without an enclosing instance"),
                                *span,
                            ));
                        };
                        self.call_method(index, name, values, *span)
                    }
                    Expr::MemberAccess { object, name, .. } => {
                        let object_value = self.eval(object, frame, me)?;
                        match object_value {
                            Value::Object(index) => self.call_method(index, name, values, *span),
                            Value::List(items) => match (name.as_str(), values.as_slice()) {
                                ("size", []) => Ok(Value::Int(items.len() as i128)),
                                ("push", [pushed]) => {
                                    let mut items = items;
                                    items.push(pushed.clone());
                                    self.write_slot(object, Value::List(items), frame, me, *span)?;
                                    Ok(Value::Void)
                                }
                                _ => Err(self.error(
                                    "E0405",
                                    format!("list method '{name}' is not supported"),
                                    *span,
                                )),
                            },
                            _ => Err(self.error(
                                "E0402",
                                format!("cannot call method '{name}' on this value"),
                                *span,
                            )),
                        }
                    }
                    _ => Err(self.error("E0402", "call target not supported", *span)),
                }
            }
            Expr::Index { base, index, span } => {
                let base_value = self.eval(base, frame, me)?;
                let index_value = self.eval(index, frame, me)?;
                let (Value::List(items), Value::Int(i)) = (&base_value, &index_value) else {
                    return Err(self.error(
                        "E0402",
                        "indexing requires a list and an integer",
                        *span,
                    ));
                };
                let len = items.len() as i128;
                if *i < 0 || *i >= len {
                    return Err(self.error(
                        "E0404",
                        format!("list index {i} out of range (size {len})"),
                        *span,
                    ));
                }
                Ok(items[*i as usize].clone())
            }
            Expr::Binary { op, lhs, rhs, span } => match op {
                BinaryOp::LogicalAnd => {
                    let Value::Bool(l) = self.eval(lhs, frame, me)? else {
                        return Err(self.error("E0402", "operand of && is not a boolean", *span));
                    };
                    if !l {
                        return Ok(Value::Bool(false));
                    }
                    let Value::Bool(r) = self.eval(rhs, frame, me)? else {
                        return Err(self.error("E0402", "operand of && is not a boolean", *span));
                    };
                    Ok(Value::Bool(r))
                }
                BinaryOp::LogicalOr => {
                    let Value::Bool(l) = self.eval(lhs, frame, me)? else {
                        return Err(self.error("E0402", "operand of || is not a boolean", *span));
                    };
                    if l {
                        return Ok(Value::Bool(true));
                    }
                    let Value::Bool(r) = self.eval(rhs, frame, me)? else {
                        return Err(self.error("E0402", "operand of || is not a boolean", *span));
                    };
                    Ok(Value::Bool(r))
                }
                _ => {
                    let l = self.eval(lhs, frame, me)?;
                    let r = self.eval(rhs, frame, me)?;
                    self.apply_binary(*op, l, r, *span)
                }
            },
            Expr::Unary {
                op,
                expr: inner,
                span,
            } => {
                let value = self.eval(inner, frame, me)?;
                match (op, value) {
                    (UnaryOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
                    (UnaryOp::Negate, Value::Int(i)) => Ok(Value::Int(-i)),
                    (UnaryOp::Negate, Value::Real(r)) => Ok(Value::Real(-r)),
                    (UnaryOp::Plus, value @ (Value::Int(_) | Value::Real(_))) => Ok(value),
                    (UnaryOp::BitNot, Value::Int(i)) => Ok(Value::Int(!i)),
                    _ => Err(self.error("E0402", "invalid operand for unary operator", *span)),
                }
            }
            Expr::Paren { expr, .. } => self.eval(expr, frame, me),
        }
    }

    fn apply_binary(
        &mut self,
        op: BinaryOp,
        lhs: Value,
        rhs: Value,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        use BinaryOp::*;
        let err = |kind: &str| {
            self.error(
                "E0402",
                format!("invalid operand ({kind}) for this operator"),
                span,
            )
        };
        match op {
            Eq | NotEq | IdentityEq | IdentityNe => {
                let equal = match (&lhs, &rhs) {
                    (Value::Int(a), Value::Int(b)) => a == b,
                    (Value::Real(a), Value::Real(b)) => a == b,
                    (Value::Int(a), Value::Real(b)) | (Value::Real(b), Value::Int(a)) => {
                        *a as f64 == *b
                    }
                    (Value::Bool(a), Value::Bool(b)) => a == b,
                    (Value::Str(a), Value::Str(b)) => a == b,
                    (
                        Value::Enum {
                            type_name: ta,
                            name: na,
                        },
                        Value::Enum {
                            type_name: tb,
                            name: nb,
                        },
                    ) => ta == tb && na == nb,
                    (Value::Object(a), Value::Object(b)) => a == b,
                    _ => return Err(err("comparison")),
                };
                // `===`/`!==` are treated as `==`/`!=` in this subset: value
                // identity for references is not distinguished yet.
                Ok(Value::Bool(match op {
                    Eq | IdentityEq => equal,
                    _ => !equal,
                }))
            }
            Lt | Le | Gt | Ge => {
                use std::cmp::Ordering::*;
                let ordering = match (&lhs, &rhs) {
                    (Value::Int(a), Value::Int(b)) => a.cmp(b),
                    (Value::Real(a), Value::Real(b)) => {
                        a.partial_cmp(b).ok_or_else(|| err("comparison"))?
                    }
                    (Value::Int(a), Value::Real(b)) => (*a as f64)
                        .partial_cmp(b)
                        .ok_or_else(|| err("comparison"))?,
                    (Value::Real(a), Value::Int(b)) => a
                        .partial_cmp(&(*b as f64))
                        .ok_or_else(|| err("comparison"))?,
                    (Value::Str(a), Value::Str(b)) => a.cmp(b),
                    _ => return Err(err("comparison")),
                };
                Ok(Value::Bool(match op {
                    Lt => ordering == Less,
                    Le => ordering != Greater,
                    Gt => ordering == Greater,
                    Ge => ordering != Less,
                    _ => unreachable!(),
                }))
            }
            Add | Sub | Mul => match (&lhs, &rhs) {
                (Value::Int(a), Value::Int(b)) => {
                    let value = match op {
                        Add => a.wrapping_add(*b),
                        Sub => a.wrapping_sub(*b),
                        Mul => a.wrapping_mul(*b),
                        _ => unreachable!(),
                    };
                    Ok(Value::Int(value))
                }
                _ => {
                    let Some(a) = as_f64(&lhs) else {
                        return Err(err("arithmetic"));
                    };
                    let Some(b) = as_f64(&rhs) else {
                        return Err(err("arithmetic"));
                    };
                    let value = match op {
                        Add => a + b,
                        Sub => a - b,
                        Mul => a * b,
                        _ => unreachable!(),
                    };
                    Ok(Value::Real(value))
                }
            },
            Div | Mod => match (&lhs, &rhs) {
                (Value::Int(a), Value::Int(b)) => {
                    if *b == 0 {
                        return Err(self.error("E0403", "division by zero", span));
                    }
                    let value = if op == Div { a / b } else { a % b };
                    Ok(Value::Int(value))
                }
                _ => {
                    let Some(a) = as_f64(&lhs) else {
                        return Err(err("arithmetic"));
                    };
                    let Some(b) = as_f64(&rhs) else {
                        return Err(err("arithmetic"));
                    };
                    Ok(Value::Real(if op == Div { a / b } else { a % b }))
                }
            },
            BitAnd | BitOr | BitXor | ShiftLeft | ShiftRight => {
                let (Value::Int(a), Value::Int(b)) = (&lhs, &rhs) else {
                    return Err(err("bitwise"));
                };
                if *b < 0 || *b >= 128 {
                    return Err(self.error("E0402", "shift amount out of range", span));
                }
                let value = match op {
                    BitAnd => a & b,
                    BitOr => a | b,
                    BitXor => a ^ b,
                    ShiftLeft => a.wrapping_shl(*b as u32),
                    ShiftRight => a.wrapping_shr(*b as u32),
                    _ => unreachable!(),
                };
                Ok(Value::Int(value))
            }
            LogicalAnd | LogicalOr => unreachable!("handled with short-circuiting"),
        }
    }

    fn read_slot(
        &mut self,
        target: &Expr,
        frame: &mut Frame,
        me: Option<usize>,
    ) -> Result<Value, Diagnostic> {
        match target {
            Expr::Ident { name, .. } => {
                if let Some(value) = frame.locals.get(name) {
                    return Ok(value.clone());
                }
                if let Some(index) = me
                    && let Some(value) = self.instances[index].get(name)
                {
                    return Ok(value.clone());
                }
                Err(self.error(
                    "E0401",
                    format!("unknown identifier '{name}'"),
                    target.span(),
                ))
            }
            Expr::MemberAccess { object, name, .. } => {
                let object_value = self.eval(object, frame, me)?;
                let Value::Object(index) = object_value else {
                    return Err(self.error(
                        "E0402",
                        "member access requires an object",
                        target.span(),
                    ));
                };
                self.instances[index].get(name).cloned().ok_or_else(|| {
                    self.error("E0401", format!("unknown field '{name}'"), target.span())
                })
            }
            _ => Err(self.error("E0402", "unsupported assignment target", target.span())),
        }
    }

    fn write_slot(
        &mut self,
        target: &Expr,
        value: Value,
        frame: &mut Frame,
        me: Option<usize>,
        span: crate::ast::Span,
    ) -> Result<(), Diagnostic> {
        match target {
            Expr::Ident { name, .. } => {
                if frame.locals.contains_key(name) {
                    frame.locals.insert(name.clone(), value);
                    return Ok(());
                }
                if let Some(index) = me {
                    if self.instances[index].set(name, value) {
                        return Ok(());
                    }
                    return Err(self.error("E0401", format!("unknown field '{name}'"), span));
                }
                Err(self.error("E0401", format!("unknown identifier '{name}'"), span))
            }
            Expr::MemberAccess { object, name, .. } => {
                let object_value = self.eval(object, frame, me)?;
                let Value::Object(index) = object_value else {
                    return Err(self.error("E0402", "member assignment requires an object", span));
                };
                if self.instances[index].set(name, value) {
                    Ok(())
                } else {
                    Err(self.error("E0401", format!("unknown field '{name}'"), span))
                }
            }
            _ => Err(self.error("E0402", "unsupported assignment target", span)),
        }
    }

    fn format_value(&self, value: &Value) -> String {
        match value {
            Value::Int(i) => i.to_string(),
            Value::Real(f) => {
                if f.is_finite() && f.fract() == 0.0 {
                    format!("{f:.1}")
                } else {
                    f.to_string()
                }
            }
            Value::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_owned(),
            Value::Str(s) => s.clone(),
            Value::Enum { name, .. } => name.clone(),
            Value::List(items) => {
                let parts: Vec<String> = items.iter().map(|item| self.format_value(item)).collect();
                format!("[{}]", parts.join(", "))
            }
            Value::Object(index) => {
                let instance = &self.instances[*index];
                let parts: Vec<String> = instance
                    .fields
                    .iter()
                    .map(|(name, value)| format!("{name}: {}", self.format_value(value)))
                    .collect();
                format!("{}{{{}}}", instance.type_name, parts.join(", "))
            }
            Value::Void => "void".to_owned(),
        }
    }

    fn value_kind(&self, value: &Value) -> &'static str {
        match value {
            Value::Int(_) | Value::Real(_) => "a number",
            Value::Bool(_) => "a boolean",
            Value::Str(_) => "a string",
            Value::Enum { .. } => "an enumeral",
            Value::List(_) => "a list",
            Value::Object(_) => "an object",
            Value::Void => "void",
        }
    }

    fn error(
        &self,
        code: &'static str,
        message: impl Into<String>,
        span: crate::ast::Span,
    ) -> Diagnostic {
        Diagnostic::new(code, message, span.0, span.1)
    }
}

/// Bits of a width specification: the literal value, or the given default
/// when the width is absent or not a literal. Capped at 64.
fn width_bits(width: &Option<Box<Expr>>, default_bits: u32) -> u32 {
    match width.as_deref() {
        Some(Expr::IntLiteral { text, .. }) => parse_int_literal(text)
            .map(|v| v.clamp(1, 64) as u32)
            .unwrap_or(default_bits),
        _ => default_bits,
    }
}

fn as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Int(i) => Some(*i as f64),
        Value::Real(r) => Some(*r),
        _ => None,
    }
}

/// Parse an integer literal: decimal, `0x`/`0b`/`0o` prefixed, or a sized/
/// unsized based literal (`8'hFF`, `'hFF`). Underscores are ignored; the
/// size prefix is ignored (widths are not truncated yet).
pub(crate) fn parse_int_literal(text: &str) -> Option<i128> {
    let lower = text.to_ascii_lowercase();
    let cleaned: String = lower.chars().filter(|c| *c != '_').collect();
    for (prefix, radix) in [("0x", 16u32), ("0b", 2), ("0o", 8)] {
        if let Some(digits) = cleaned.strip_prefix(prefix) {
            return i128::from_str_radix(digits, radix).ok();
        }
    }
    if let Some(quote_pos) = cleaned.find('\'') {
        let mut rest = cleaned[quote_pos + 1..].chars();
        let base = rest.next()?;
        let digits: &str = rest.as_str();
        let radix = match base {
            'b' => 2u32,
            'o' => 8,
            'd' => 10,
            'x' | 'h' | 'f' => 16,
            _ => return None,
        };
        return i128::from_str_radix(digits, radix).ok();
    }
    cleaned.parse::<i128>().ok()
}

/// Interpret the escape sequences the lexer preserved as raw text.
fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some('0') => out.push('\0'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}
