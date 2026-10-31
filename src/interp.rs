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
//! - `E0408` simulator bridge error from the `hdl_*` routines
//! - `E0409` cycle budget exceeded (runaway TCM)
//! - `E0410` unsupported string-match pattern shape (`/.../` or a
//!   native meta-character)
//!
//! `emit` records the firing and immediately executes the event's `on`
//! action blocks (clause 11.2), in composition order, on the instance
//! the event path resolves to; `@sim`-attached events never fire.

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
    /// `simple_port` members: port name → HDL location for the bridge.
    ports: Vec<(String, String)>,
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

    /// The HDL location bound to a port, if any.
    fn port_path(&self, name: &str) -> Option<&str> {
        self.ports
            .iter()
            .find(|(port, _)| port == name)
            .map(|(_, path)| path.as_str())
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
    /// `stop_run()` — unwind to the top of the run.
    Stop,
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
    /// Live simulator session for the `hdl_*` builtins.
    hdl: Option<crate::hdl::HdlSession>,
    /// Virtual simulation time in cycles, advanced by `wait` statements.
    cycle: u64,
    /// Remaining `wait` statements before a runaway TCM is stopped.
    wait_budget: u64,
    /// Events fired by `emit`, keyed by their path as written.
    emitted: std::collections::BTreeSet<String>,
    /// Set by `stop_run()`; unwinds the whole run.
    stop_requested: bool,
    /// Failed `check that` conditions (`dut_error`), reported like
    /// Specman's DUT-error count.
    dut_errors: usize,
    /// `define` constants resolved to values.
    defines: std::collections::BTreeMap<String, Value>,
}

/// How many `wait` statements one run may execute (the runaway-TCM guard).
const WAIT_BUDGET: u64 = 100_000;

/// Instantiate the built-in `sys` unit and execute its `run()` methods.
pub fn run(elaborated: &Elaborated, seed: u64) -> RunResult {
    // Resolve `define` constants (literal values only; non-literals are
    // skipped — they need the elaboration-time substitution that a macro
    // pass would provide).
    let defines: std::collections::BTreeMap<String, Value> = elaborated
        .defines
        .iter()
        .filter_map(|(name, expr)| {
            let value = match expr {
                Expr::IntLiteral { text, .. } => parse_int_literal(text).map(Value::Int),
                Expr::BitLiteral { text, .. } => parse_int_literal(text).map(Value::Int),
                Expr::StringLiteral { value, .. } => Some(Value::Str(unescape(value))),
                Expr::BoolLiteral { value, .. } => Some(Value::Bool(*value)),
                _ => None,
            }?;
            Some((name.clone(), value))
        })
        .collect();
    let mut interp = Interp {
        types: elaborated.types.as_slice(),
        instances: Vec::new(),
        rng: Rng::new(seed),
        seed,
        output: String::new(),
        depth: 0,
        hdl: None,
        cycle: 0,
        wait_budget: WAIT_BUDGET,
        emitted: std::collections::BTreeSet::new(),
        stop_requested: false,
        dut_errors: 0,
        defines,
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
    let sys_index = interp.instantiate("sys");
    // Specman generates the root unit's data fields before run() starts
    // (respecting `keep` constraints and skipping `!` fields); mirror
    // that so corpus testbenches see generated lists/scalars. Both this
    // and the sequence start happen even when no explicit run() exists.
    if let Err(diagnostic) = interp.gen_object(sys_index) {
        interp
            .output
            .push_str(&format!("implicit gen: {}\n", diagnostic.render()));
    }
    // Start each declared sequence's MAIN body inline: Specman's
    // sequencer launches MAIN during test start; the inline subset runs
    // it synchronously with `driver` bound to the driver instance.
    let sequences: Vec<crate::elab::SequenceInfo> = elaborated.sequences.clone();
    for sequence in &sequences {
        let Some(driver_index) = interp
            .instances
            .iter()
            .position(|instance| instance.type_name == sequence.driver)
        else {
            continue;
        };
        let sequence_index = interp.instantiate(&sequence.name);
        interp.instances[sequence_index].set("driver", Value::Object(driver_index));
        let types: &[crate::elab::ElaboratedType] = interp.types;
        let Some(body) = types
            .iter()
            .find(|t| t.name == sequence.name)
            .and_then(|t| t.methods.iter().find(|m| m.name == "body").cloned())
        else {
            continue;
        };
        let mut frame = Frame::default();
        frame
            .locals
            .insert("driver".to_owned(), Value::Object(driver_index));
        if let Err(diagnostic) = interp.exec_block(&body.body, &mut frame, Some(sequence_index)) {
            interp.output.push_str(&format!(
                "sequence {}: {}\n",
                sequence.name,
                diagnostic.render()
            ));
        }
    }
    let mut diagnostics = Vec::new();
    for block in run_blocks {
        let mut frame = Frame::default();
        if let Err(diagnostic) = interp.exec_block(&block.body, &mut frame, Some(sys_index)) {
            diagnostics.push(diagnostic);
            break;
        }
    }
    // Specman closes every run with a DUT-error count; mirror it.
    if interp.dut_errors > 0 {
        interp.output.push_str(&format!(
            "Checking is complete - {} DUT error(s).\n",
            interp.dut_errors
        ));
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
        let mut ports = Vec::new();
        let types: &[crate::elab::ElaboratedType] = self.types;
        if let Some(ty) = types.iter().find(|t| t.name == type_name) {
            for field in &ty.fields {
                let default = self.default_for(&field.ty);
                fields.push((field.name.clone(), default));
            }
            for port in &ty.ports {
                if let Some(hdl_path) = &port.hdl_path {
                    ports.push((port.name.clone(), hdl_path.clone()));
                }
            }
        }
        self.instances.push(ObjectInstance {
            type_name: type_name.to_owned(),
            fields,
            ports,
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
            TypeExpr::InlineEnum { names, .. } => {
                // The default enumerand is the first one declared.
                match names.first() {
                    Some(first) => Value::Enum {
                        type_name: inline_enum_key(names),
                        name: first.clone(),
                    },
                    None => Value::Void,
                }
            }
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
                    Expr::HdlPath { path, span } => {
                        if self.hdl.is_none() {
                            return Err(self.error(
                                "E0408",
                                "no simulator loaded — call hdl_load first",
                                *span,
                            ));
                        }
                        let Some(int_value) = as_i64(&value) else {
                            return Err(self.error(
                                "E0402",
                                "signal assignment expects an integer (or boolean) value",
                                *span,
                            ));
                        };
                        let session = self.hdl.as_mut().unwrap();
                        match session.set(path, int_value) {
                            Ok(()) => Ok(Flow::Normal),
                            Err(msg) => Err(self.error("E0408", msg, *span)),
                        }
                    }
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
                        Flow::Stop => return Ok(Flow::Stop),
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
            Stmt::StopRun { .. } => {
                self.stop_requested = true;
                Ok(Flow::Stop)
            }
            Stmt::SeqDo { item, span } => {
                // Inline item execution: gen the item field in place.
                // The sequencer handshake (get_next_item/item_done
                // blocking) needs the TCM scheduler — a later milestone.
                self.exec_gen(
                    &Expr::Ident {
                        name: item.clone(),
                        span: *span,
                    },
                    frame,
                    me,
                    *span,
                )
            }
            Stmt::Check {
                condition,
                else_body,
                span,
            } => {
                let Value::Bool(holds) = self.eval(condition, frame, me)? else {
                    return Err(self.error(
                        "E0402",
                        "the condition of 'check that' is not a boolean",
                        *span,
                    ));
                };
                if !holds {
                    // Specman counts DUT errors and keeps running; the
                    // else body (typically `dut_error(...)`) reports.
                    self.dut_errors += 1;
                    self.exec_block(else_body, frame, me)?;
                }
                Ok(Flow::Normal)
            }
            Stmt::ForEach {
                var,
                list,
                body,
                span,
            } => {
                let list_value = self.eval(list, frame, me)?;
                match list_value {
                    Value::List(items) => {
                        let binding = var.clone().unwrap_or_else(|| "it".to_owned());
                        for item in items {
                            frame.locals.insert(binding.clone(), item);
                            match self.exec_block(body, frame, me)? {
                                Flow::Normal => {}
                                Flow::Break => break,
                                Flow::Continue => continue,
                                other => return Ok(other),
                            }
                        }
                        Ok(Flow::Normal)
                    }
                    other => Err(self.error(
                        "E0402",
                        format!("for-each expects a list, got {}", self.value_kind(&other)),
                        *span,
                    )),
                }
            }
            Stmt::ForTo {
                var,
                from,
                to,
                body,
                span,
            } => {
                let Value::Int(start_v) = self.eval(from, frame, me)? else {
                    return Err(self.error("E0402", "for lower bound must be an integer", *span));
                };
                let Value::Int(end_v) = self.eval(to, frame, me)? else {
                    return Err(self.error("E0402", "for upper bound must be an integer", *span));
                };
                let mut i = start_v;
                while i <= end_v {
                    frame.locals.insert(var.clone(), Value::Int(i));
                    match self.exec_block(body, frame, me)? {
                        Flow::Normal => {}
                        Flow::Break => break,
                        Flow::Continue => continue,
                        other => return Ok(other),
                    }
                    i += 1;
                }
                Ok(Flow::Normal)
            }
            Stmt::Wait { cycles, span } => {
                let Value::Int(n) = self.eval(cycles, frame, me)? else {
                    return Err(self.error("E0402", "wait count must be an integer", *span));
                };
                if n < 0 {
                    return Err(self.error("E0402", "wait count must be non-negative", *span));
                }
                if self.wait_budget == 0 {
                    return Err(self.error(
                        "E0409",
                        format!("cycle budget exceeded at cycle {}", self.cycle),
                        *span,
                    ));
                }
                self.wait_budget -= 1;
                self.cycle = self.cycle.saturating_add(n as u64);
                Ok(Flow::Normal)
            }
            Stmt::Emit { target, span } => {
                let path = self.event_path(target)?;
                self.emitted.insert(path.clone());
                // `emit` executes the event's `on` action blocks
                // immediately, in declaration order (clause 11.2). No
                // TCMs wait in this milestone, so "before waiting TCMs"
                // is trivially satisfied; `@sim`-attached events never
                // fire here.
                let owner = self.resolve_event_owner(target, frame, me);
                self.run_on_blocks(&path, owner, *span)?;
                Ok(Flow::Normal)
            }
            Stmt::Start { call, span } => {
                // TCM start: executed inline (synchronously) in this
                // milestone — event-triggered suspension is a later
                // milestone; waits inside the body advance virtual time.
                let Expr::Call { callee, args, .. } = call else {
                    return Err(self.error("E0402", "start expects a method call", *span));
                };
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(self.eval(arg, frame, me)?);
                }
                match callee.as_ref() {
                    Expr::Ident { name, .. } => {
                        let Some(index) = me else {
                            return Err(self.error(
                                "E0402",
                                format!("start of '{name}' without an enclosing instance"),
                                *span,
                            ));
                        };
                        self.call_method(index, name, values, *span)?;
                    }
                    Expr::MemberAccess { object, name, .. } => {
                        let object_value = self.eval(object, frame, me)?;
                        let Value::Object(index) = object_value else {
                            return Err(self.error("E0402", "start requires an object", *span));
                        };
                        self.call_method(index, name, values, *span)?;
                    }
                    _ => return Err(self.error("E0402", "start target not supported", *span)),
                }
                Ok(Flow::Normal)
            }
            Stmt::Case {
                scrutinee,
                arms,
                default,
                span,
            } => {
                let value = self.eval(scrutinee, frame, me)?;
                let mut matched: Option<&Vec<Stmt>> = None;
                'arms: for (alternatives, body) in arms {
                    for alternative in alternatives {
                        let alt_value = self.eval(alternative, frame, me)?;
                        if matches!(
                            self.apply_binary(BinaryOp::Eq, value.clone(), alt_value, *span)?,
                            Value::Bool(true)
                        ) {
                            matched = Some(body);
                            break 'arms;
                        }
                    }
                }
                match matched.or(default.as_ref()) {
                    Some(body) => self.exec_block(body, frame, me),
                    None => Ok(Flow::Normal),
                }
            }
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
            .map(|t| {
                t.constraints
                    .iter()
                    .filter(|c| !c.is_bind)
                    .map(|c| c.expr.clone())
                    .collect()
            })
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
                    crate::solve::SolveOutcome::Unsupported => {}
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
    /// Whether the type expression names a unit (instance) type — those
    /// fields are instantiated, never generated.
    fn is_unit_typed(&self, ty: &TypeExpr) -> bool {
        let TypeExpr::Named { name, .. } = ty else {
            return false;
        };
        self.types
            .iter()
            .any(|t| &t.name == name && t.kind == DeclarationKind::Unit)
    }

    /// The fixed length demanded by a `keep <field>.size() == N`
    /// constraint, if any (only the equality form drives lengths here;
    /// ranges are a later milestone).
    fn required_list_size<'c>(&self, field: &str, constraints: &'c [Expr]) -> Option<usize> {
        for constraint in constraints {
            let Expr::Binary {
                op: crate::ast::BinaryOp::Eq,
                lhs,
                rhs,
                ..
            } = constraint
            else {
                continue;
            };
            let size_call = |expr: &'c Expr| -> Option<(&'c String, &'c Expr)> {
                let Expr::Call { callee, args, .. } = expr else {
                    return None;
                };
                if !args.is_empty() {
                    return None;
                }
                let Expr::MemberAccess { object, name, .. } = callee.as_ref() else {
                    return None;
                };
                if name != "size" {
                    return None;
                }
                let Expr::Ident { name, .. } = object.as_ref() else {
                    return None;
                };
                Some((name, expr))
            };
            let (accessor, size_expr) = if let Some((name, _)) = size_call(lhs) {
                (name, rhs.as_ref())
            } else if let Some((name, _)) = size_call(rhs) {
                (name, lhs.as_ref())
            } else {
                continue;
            };
            if accessor != field {
                continue;
            }
            if let Expr::IntLiteral { text, .. } = size_expr
                && let Some(length) = parse_int_literal(text)
                && let Ok(length) = usize::try_from(length)
            {
                return Some(length);
            }
        }
        None
    }

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
        // `!` fields keep their value; instance (unit-typed) fields are
        // not generated — they are instantiated structurally.
        let field_types: Vec<(String, TypeExpr)> = ty
            .fields
            .iter()
            .filter(|field| !field.no_gen && !self.is_unit_typed(&field.ty))
            .map(|field| (field.name.clone(), field.ty.clone()))
            .collect();
        let constraints: Vec<Expr> = ty
            .constraints
            .iter()
            .filter(|constraint| !constraint.is_bind)
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
                // A `keep list.size() == N` constraint drives the list
                // length: fill the sampled (empty) list with generated
                // elements so the constraint can pass.
                if let Some(length) = self.required_list_size(name, &constraints)
                    && let Value::List(items) = self.instances[index]
                        .get(name)
                        .cloned()
                        .unwrap_or(Value::Void)
                {
                    let mut filled = items;
                    filled.resize(length, Value::Void);
                    for slot in filled.iter_mut() {
                        let element_ty = list_element_type(field_ty);
                        let element = self.sample_type(&element_ty)?;
                        *slot = element;
                        if let Value::Object(element_index) = *slot {
                            self.gen_object(element_index)?;
                        }
                    }
                    self.instances[index].set(name, Value::List(filled));
                }
            }
            let mut pass = true;
            for constraint in &constraints {
                // HDL-binding constraints (`keep x.hdl_path() == "…"`,
                // `keep x.delayed() == FALSE`) record simulator-side
                // facts, not data constraints; they never gate gen.
                if expr_calls_any(constraint, &["hdl_path", "delayed", "soft", "select"]) {
                    continue;
                }
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
            crate::solve::SolveOutcome::Unsupported => Ok(None),
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
            TypeExpr::InlineEnum { names, .. } => {
                // Random enumerand from the inline list.
                if names.is_empty() {
                    return Ok(Value::Void);
                }
                let pick = names[self.rng.below(names.len() as u128) as usize].clone();
                Ok(Value::Enum {
                    type_name: inline_enum_key(names),
                    name: pick,
                })
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
                // stop_run() inside a TCM unwinds the whole run; the flag
                // is checked by the run loop and the calling scheduler.
                Flow::Stop => {
                    self.stop_requested = true;
                }
            }
        }
        self.depth -= 1;
        Ok(result)
    }

    /// Dispatch a built-in routine call.
    fn call_builtin(
        &mut self,
        name: &str,
        values: Vec<Value>,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        if name == "out" {
            let text: String = values.iter().map(|v| self.format_value(v)).collect();
            self.output.push_str(&text);
            return Ok(Value::Void);
        }
        if name == "outf" {
            return self.call_outf(values, span);
        }
        if name == "dut_error" || name == "error" {
            self.dut_errors += 1;
            let text: String = values.iter().map(|v| self.format_value(v)).collect();
            self.output.push_str(&format!("{name}: {text}\n"));
            return Ok(Value::Void);
        }
        if name == "message" {
            // message(severity, ...): the severity argument is skipped in
            // this subset (verbosity filtering is not modeled).
            let text: String = values
                .iter()
                .skip(1)
                .map(|v| self.format_value(v))
                .collect();
            self.output.push_str(&text);
            self.output.push('\n');
            return Ok(Value::Void);
        }
        if name == "append" {
            // String concatenation form; list append is a later milestone.
            let text: String = values.iter().map(|v| self.format_value(v)).collect();
            return Ok(Value::Str(text));
        }
        if name == "stop_run" {
            self.stop_requested = true;
            return Ok(Value::Void);
        }
        if name == "raise_objection" || name == "drop_objection" {
            // Objections coordinate end-of-test across TCMs; the inline
            // subset has nothing to wait on, so they are no-ops.
            return Ok(Value::Void);
        }
        if name == "pack" {
            // pack(NULL, item) packs the item's physical fields,
            // MSB-first in declaration order, into one integer. Only the
            // null-packing argument form occurs in the corpora; the
            // packing-options argument is ignored.
            let Some(Value::Object(index)) =
                values.iter().rev().find(|v| matches!(v, Value::Object(_)))
            else {
                return Err(self.error("E0402", "'pack' expects an object to pack", span));
            };
            return self.pack_object(*index, span);
        }
        if name.starts_with("hdl_") {
            return self.call_hdl(name, values, span);
        }
        Err(self.error("E0405", format!("routine '{name}' is not supported"), span))
    }

    /// `pack(NULL, obj)` — concatenate the object's physical fields
    /// MSB-first into one integer (per-field widths from their scalar
    /// declarations; non-physical and `!` fields are excluded).
    fn pack_object(&mut self, index: usize, span: crate::ast::Span) -> Result<Value, Diagnostic> {
        let type_name = self.instances[index].type_name.clone();
        let types: &[crate::elab::ElaboratedType] = self.types;
        let Some(ty) = types.iter().find(|t| t.name == type_name) else {
            return Err(self.error("E0401", format!("unknown type '{type_name}'"), span));
        };
        let mut packed: i128 = 0;
        for field in &ty.fields {
            if !field.physical || field.no_gen {
                continue;
            }
            let Some(value) = self.instances[index].get(&field.name) else {
                continue;
            };
            let Value::Int(bits) = value else {
                return Err(self.error(
                    "E0402",
                    format!("field '{}' cannot be packed (not an integer)", field.name),
                    span,
                ));
            };
            let width = field_width_bits(&field.ty).unwrap_or(1) as u32;
            packed = (packed << width) | (bits & ((1i128 << width) - 1));
        }
        Ok(Value::Int(packed))
    }

    /// `outf("format %x", args)` — printf-style output; specs `%x`/`%X`
    /// (hex), `%d` (decimal) and `%s` (value as text) consume arguments in
    /// order; `%%` is a literal percent.
    fn call_outf(
        &mut self,
        values: Vec<Value>,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        let Some((format, arguments)) = values.split_first() else {
            return Err(self.error(
                "E0402",
                "'outf' expects a format string and arguments",
                span,
            ));
        };
        let Value::Str(format) = format else {
            return Err(self.error("E0402", "'outf' expects a format string first", span));
        };
        let mut argument_index = 0usize;
        let mut rendered = String::new();
        let mut chars = format.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '%' {
                rendered.push(c);
                continue;
            }
            match chars.next() {
                Some('%') => rendered.push('%'),
                Some(spec) => {
                    let Some(value) = arguments.get(argument_index) else {
                        return Err(self.error(
                            "E0402",
                            format!("'outf': no argument for '%{spec}'"),
                            span,
                        ));
                    };
                    argument_index += 1;
                    rendered.push_str(&match (spec, value) {
                        ('x', Value::Int(i)) => format!("{i:x}"),
                        ('X', Value::Int(i)) => format!("{i:X}"),
                        ('d', Value::Int(i)) => format!("{i}"),
                        (_, value) => self.format_value(value),
                    });
                }
                None => rendered.push('%'),
            }
        }
        self.output.push_str(&rendered);
        Ok(Value::Void)
    }

    /// Resolve an `emit`/event target expression to its path key.
    fn event_path(&mut self, target: &Expr) -> Result<String, Diagnostic> {
        match target {
            Expr::Ident { name, .. } => Ok(name.clone()),
            Expr::MemberAccess { object, name, .. } => {
                let mut prefix = self.event_path(object)?;
                prefix.push('.');
                prefix.push_str(name);
                Ok(prefix)
            }
            _ => Err(self.error("E0402", "unsupported event target", target.span())),
        }
    }

    /// The instance whose `on` blocks an emitted event triggers: the
    /// enclosing instance for a bare event name (the event is local to
    /// its struct, clause 11.2), or the object the dotted path reads
    /// through. `None` when no instance resolves — the firing is still
    /// recorded, but no `on` blocks run.
    fn resolve_event_owner(
        &mut self,
        target: &Expr,
        frame: &mut Frame,
        me: Option<usize>,
    ) -> Option<usize> {
        match target {
            Expr::Ident { .. } => me,
            Expr::MemberAccess { object, .. } => {
                let object_value = self.eval(object, frame, me).ok()?;
                match object_value {
                    Value::Object(index) => Some(index),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Execute the `on` action blocks (clause 11.2) registered for an
    /// emitted event path, in composition order — base types first, then
    /// extends as written. The event is local to the owner's struct, so
    /// the match is on the event name (last path segment) within the
    /// owner's type; each block runs with a fresh locals frame on the
    /// owning instance, and `return`/`break`/`continue` end that block
    /// only. The depth guard bounds `emit`-inside-`on` recursion.
    fn run_on_blocks(
        &mut self,
        path: &str,
        owner: Option<usize>,
        span: crate::ast::Span,
    ) -> Result<(), Diagnostic> {
        let Some(owner) = owner else {
            return Ok(());
        };
        let event_name = path.rsplit('.').next().unwrap_or(path);
        let type_name = self.instances[owner].type_name.clone();
        let Some(ty) = self.types.iter().find(|t| t.name == type_name) else {
            return Ok(());
        };
        let blocks: Vec<crate::ast::OnDecl> = ty
            .ons
            .iter()
            .filter(|on| on.path.last().map(String::as_str) == Some(event_name))
            .cloned()
            .collect();
        if blocks.is_empty() {
            return Ok(());
        }
        if self.depth > 200 {
            return Err(self.error("E0406", "recursion too deep", span));
        }
        self.depth += 1;
        for block in &blocks {
            let mut frame = Frame::default();
            match self.exec_block(&block.body, &mut frame, Some(owner))? {
                Flow::Normal | Flow::Break | Flow::Continue | Flow::Return(_) => {}
                Flow::Stop => {
                    self.stop_requested = true;
                }
            }
        }
        self.depth -= 1;
        Ok(())
    }

    /// The `hdl_*` routines: drive the external SystemVerilog simulator
    /// through [`crate::hdl`]. See examples/mixed_counter.e.
    fn call_hdl(
        &mut self,
        name: &str,
        values: Vec<Value>,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        // Diagnostics are built inline (no closure holds a borrow of
        // `self` across the mutable session accesses).
        let not_loaded = || {
            Diagnostic::new(
                "E0408",
                "no simulator loaded — call hdl_load first",
                span.0,
                span.1,
            )
        };
        match name {
            "hdl_load" => {
                if values.len() != 2 {
                    return Err(self.error(
                        "E0402",
                        format!("'hdl_load' expects 2 arguments, got {}", values.len()),
                        span,
                    ));
                }
                let [file, top] = values.as_slice() else {
                    unreachable!("checked the count");
                };
                let Value::Str(file) = file else {
                    return Err(self.error("E0402", "hdl_load expects a design file string", span));
                };
                let Value::Str(top) = top else {
                    return Err(self.error(
                        "E0402",
                        "hdl_load expects a top module name string",
                        span,
                    ));
                };
                match crate::hdl::HdlSession::load(file, top) {
                    Ok(session) => {
                        self.hdl = Some(session);
                        Ok(Value::Void)
                    }
                    Err(msg) => Err(self.error("E0408", msg, span)),
                }
            }
            "hdl_set" => {
                if values.len() != 2 {
                    return Err(self.error(
                        "E0402",
                        format!("'hdl_set' expects 2 arguments, got {}", values.len()),
                        span,
                    ));
                }
                let [path, value] = values.as_slice() else {
                    unreachable!("checked the count");
                };
                let Value::Str(path) = path else {
                    return Err(self.error("E0402", "hdl_set expects a signal path string", span));
                };
                let Some(value) = as_i64(value) else {
                    return Err(self.error(
                        "E0402",
                        "hdl_set expects an integer (or boolean) value",
                        span,
                    ));
                };
                if self.hdl.is_none() {
                    return Err(not_loaded());
                }
                match self.hdl.as_mut().unwrap().set(path, value) {
                    Ok(()) => Ok(Value::Void),
                    Err(msg) => Err(self.error("E0408", msg, span)),
                }
            }
            "hdl_get" => {
                if values.len() != 1 {
                    return Err(self.error(
                        "E0402",
                        format!("'hdl_get' expects 1 argument, got {}", values.len()),
                        span,
                    ));
                }
                let [path] = values.as_slice() else {
                    unreachable!("checked the count");
                };
                let Value::Str(path) = path else {
                    return Err(self.error("E0402", "hdl_get expects a signal path string", span));
                };
                if self.hdl.is_none() {
                    return Err(not_loaded());
                }
                match self.hdl.as_mut().unwrap().get(path) {
                    Ok(value) => Ok(Value::Int(value)),
                    Err(msg) => Err(self.error("E0408", msg, span)),
                }
            }
            "hdl_run" => {
                if values.len() != 1 {
                    return Err(self.error(
                        "E0402",
                        format!("'hdl_run' expects 1 argument, got {}", values.len()),
                        span,
                    ));
                }
                let [delay] = values.as_slice() else {
                    unreachable!("checked the count");
                };
                let Some(delay) = as_i64(delay) else {
                    return Err(self.error("E0402", "hdl_run expects an integer delay", span));
                };
                if delay < 0 {
                    return Err(self.error("E0402", "hdl_run expects a non-negative delay", span));
                }
                if self.hdl.is_none() {
                    return Err(not_loaded());
                }
                match self.hdl.as_mut().unwrap().run(delay as u64) {
                    Ok(()) => Ok(Value::Void),
                    Err(msg) => Err(self.error("E0408", msg, span)),
                }
            }
            "hdl_finish" => {
                if !values.is_empty() {
                    return Err(self.error(
                        "E0402",
                        format!("'hdl_finish' expects no arguments, got {}", values.len()),
                        span,
                    ));
                }
                if let Some(mut session) = self.hdl.take() {
                    session.finish();
                }
                Ok(Value::Void)
            }
            _ => Err(self.error("E0405", format!("routine '{name}' is not supported"), span)),
        }
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
                if based_value_is_mvl(text) {
                    return Err(self.error(
                        "E0402",
                        format!("MVL literal '{text}' is not evaluated yet"),
                        expr.span(),
                    ));
                }
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
                // `port$` — simple_port value access through the bridge.
                if let Some(port) = name.strip_suffix('$') {
                    return self.read_port_value(port, me, *span);
                }
                // `define` constants.
                if let Some(value) = self.defines.get(name) {
                    return Ok(value.clone());
                }
                // The null reference.
                if name == "NULL" {
                    return Ok(Value::Void);
                }
                // Objection identifiers: values are irrelevant to the
                // inline subset.
                if name == "TEST_DONE" {
                    return Ok(Value::Int(0));
                }
                // Message-verbosity levels.
                if matches!(name.as_str(), "LOW" | "MEDIUM" | "HIGH") {
                    let level = match name.as_str() {
                        "LOW" => 0,
                        "MEDIUM" => 1,
                        _ => 2,
                    };
                    return Ok(Value::Int(level));
                }
                if let Some(value) = frame.locals.get(name) {
                    return Ok(value.clone());
                }
                if let Some(index) = me
                    && let Some(value) = self.instances[index].get(name)
                {
                    return Ok(value.clone());
                }
                // The root unit instance is addressable by name from any
                // method (`for each in sys.ops`).
                if name == "sys" {
                    let sys_index = self
                        .instances
                        .iter()
                        .position(|instance| instance.type_name == "sys");
                    if let Some(index) = sys_index {
                        return Ok(Value::Object(index));
                    }
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
                // Inline enumeral lists on fields: `[imm, reg]` etc.
                for t in types {
                    for field in &t.fields {
                        if let TypeExpr::InlineEnum { names, .. } = &field.ty
                            && names.iter().any(|n| n == name)
                        {
                            return Ok(Value::Enum {
                                type_name: inline_enum_key(names),
                                name: name.clone(),
                            });
                        }
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
                        // `unit.port$` — simple_port value access.
                        if let Some(port) = name.strip_suffix('$') {
                            let port = port.to_owned();
                            return self.read_port_value(&port, Some(index), *span);
                        }
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
                            return self.call_builtin(name, values, *span);
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
                        // Objection/other builtins called through an
                        // instance (`driver.raise_objection(...)`) route
                        // to the builtin table first.
                        if crate::builtins::is_builtin_routine(name) {
                            return self.call_builtin(name, values, *span);
                        }
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
                // `a => b` is `!a || b`, short-circuiting on a false lhs.
                BinaryOp::Imply => {
                    let Value::Bool(l) = self.eval(lhs, frame, me)? else {
                        return Err(self.error("E0402", "operand of => is not a boolean", *span));
                    };
                    if !l {
                        return Ok(Value::Bool(true));
                    }
                    let Value::Bool(r) = self.eval(rhs, frame, me)? else {
                        return Err(self.error("E0402", "operand of => is not a boolean", *span));
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
            Expr::Ternary {
                cond,
                then_expr,
                else_expr,
                span,
            } => {
                let Value::Bool(cond_value) = self.eval(cond, frame, me)? else {
                    return Err(self.error("E0402", "ternary condition is not a boolean", *span));
                };
                if cond_value {
                    self.eval(then_expr, frame, me)
                } else {
                    self.eval(else_expr, frame, me)
                }
            }
            Expr::CastAs { expr, .. } => {
                // Casts are value-preserving in this subset: integers,
                // enumerals and booleans flow through unchanged.
                self.eval(expr, frame, me)
            }
            Expr::BitSlice {
                base,
                high,
                low,
                span,
            } => {
                let Value::Int(value) = self.eval(base, frame, me)? else {
                    return Err(self.error("E0402", "bit slice expects an integer", *span));
                };
                let Value::Int(h) = self.eval(high, frame, me)? else {
                    return Err(self.error("E0402", "slice bound must be an integer", *span));
                };
                let Value::Int(l) = self.eval(low, frame, me)? else {
                    return Err(self.error("E0402", "slice bound must be an integer", *span));
                };
                if l < 0 || h < l || h >= 127 {
                    return Err(self.error("E0402", "bit slice range invalid", *span));
                }
                let width = (h - l + 1) as u32;
                let mask: i128 = if width >= 127 {
                    -1
                } else {
                    (1i128 << width) - 1
                };
                Ok(Value::Int((value >> l) & mask))
            }
            Expr::InSet { element, set, span } => {
                let value = self.eval(element, frame, me)?;
                let contained = match set {
                    crate::ast::InSetExpr::Items(items) => {
                        let mut found = false;
                        for item in items {
                            let item_value = self.eval(item, frame, me)?;
                            if matches!(
                                self.apply_binary(BinaryOp::Eq, value.clone(), item_value, *span)?,
                                Value::Bool(true)
                            ) {
                                found = true;
                                break;
                            }
                        }
                        found
                    }
                    crate::ast::InSetExpr::Range(low, high) => {
                        let Value::Int(low_v) = self.eval(low, frame, me)? else {
                            return Err(self.error(
                                "E0402",
                                "range bound must be an integer",
                                *span,
                            ));
                        };
                        let Value::Int(high_v) = self.eval(high, frame, me)? else {
                            return Err(self.error(
                                "E0402",
                                "range bound must be an integer",
                                *span,
                            ));
                        };
                        let Value::Int(v) = &value else {
                            return Err(self.error(
                                "E0402",
                                "range membership expects an integer",
                                *span,
                            ));
                        };
                        low_v <= *v && *v <= high_v
                    }
                };
                Ok(Value::Bool(contained))
            }
            Expr::IsA {
                expr,
                ty,
                negated,
                span,
            } => {
                let value = self.eval(expr, frame, me)?;
                let is_a = self.value_is_a(&value, ty, *span)?;
                Ok(Value::Bool(is_a != *negated))
            }
            Expr::HdlPath { path, span } => {
                if self.hdl.is_none() {
                    return Err(self.error(
                        "E0408",
                        "no simulator loaded — call hdl_load first",
                        *span,
                    ));
                }
                let session = self.hdl.as_mut().unwrap();
                match session.get(path) {
                    Ok(value) => Ok(Value::Int(value)),
                    Err(msg) => Err(self.error("E0408", msg, *span)),
                }
            }
        }
    }

    /// The runtime half of `expr is [not] a <type>` (clause 4.5). Scalar
    /// kinds match by value shape (integers are one runtime value, so
    /// `is a int` and `is a uint` both accept it); named types match by
    /// declared name, walking `like` bases — subtype-aware dispatch is a
    /// later milestone.
    fn value_is_a(
        &self,
        value: &Value,
        ty: &TypeExpr,
        span: crate::ast::Span,
    ) -> Result<bool, Diagnostic> {
        match ty {
            TypeExpr::Scalar { base, .. } => Ok(matches!(
                (base, value),
                (
                    ScalarBase::Int
                        | ScalarBase::Uint
                        | ScalarBase::Bit
                        | ScalarBase::Byte
                        | ScalarBase::Nibble
                        | ScalarBase::Time,
                    Value::Int(_)
                ) | (ScalarBase::Bool, Value::Bool(_))
                    | (ScalarBase::Real, Value::Real(_))
                    | (ScalarBase::String, Value::Str(_))
            )),
            TypeExpr::List { .. } => Ok(matches!(value, Value::List(_))),
            TypeExpr::Named { name, .. } => Ok(match value {
                Value::Object(index) => self.type_is_a(&self.instances[*index].type_name, name),
                Value::Enum { type_name, .. } => self.type_is_a(type_name, name),
                _ => false,
            }),
            TypeExpr::InlineEnum { .. } => Err(self.error(
                "E0402",
                "'is a' with an inline enumeral type is not supported",
                span,
            )),
        }
    }

    /// Whether `name` is `target` or a `like` descendant of it.
    fn type_is_a(&self, name: &str, target: &str) -> bool {
        let mut current = name;
        for _ in 0..=self.types.len() {
            if current == target {
                return true;
            }
            let Some(declared) = self.types.iter().find(|t| t.name == current) else {
                return false;
            };
            match &declared.base {
                Some(base) => current = base.as_str(),
                None => return false,
            }
        }
        false
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
            Match | NotMatch => {
                let (Value::Str(text), Value::Str(pattern)) = (&lhs, &rhs) else {
                    return Err(err("string match"));
                };
                // `/.../` selects AWK-style regular expression matching
                // (clause 4.11.2) — not implemented.
                if pattern.len() >= 2 && pattern.starts_with('/') && pattern.ends_with('/') {
                    return Err(self.error(
                        "E0410",
                        "AWK-style regular expression string matching is not supported",
                        span,
                    ));
                }
                // Native matching (clause 4.11.1) is case insensitive and
                // always matches the full string. Only a literal pattern —
                // letters, digits, underscore and space — is evaluated;
                // the meta-characters of the native pattern language are
                // not implemented.
                if !pattern
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ' ')
                {
                    return Err(self.error(
                        "E0410",
                        "native string matching with meta-characters is not supported",
                        span,
                    ));
                }
                let matched = text.to_lowercase() == pattern.to_lowercase();
                Ok(Value::Bool(if op == Match { matched } else { !matched }))
            }
            LogicalAnd | LogicalOr | Imply => unreachable!("handled with short-circuiting"),
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
                if let Some(port) = name.strip_suffix('$') {
                    let port = port.to_owned();
                    return self.read_port_value(&port, me, target.span());
                }
                if let Some(value) = frame.locals.get(name) {
                    return Ok(value.clone());
                }
                if let Some(index) = me
                    && let Some(value) = self.instances[index].get(name)
                {
                    return Ok(value.clone());
                }
                // The root unit instance is addressable by name from any
                // method (`for each in sys.ops`).
                if name == "sys"
                    && let Some(index) = self
                        .instances
                        .iter()
                        .position(|instance| instance.type_name == "sys")
                {
                    return Ok(Value::Object(index));
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
                if let Some(port) = name.strip_suffix('$') {
                    let port = port.to_owned();
                    return self.write_port_value(&port, value, me, span);
                }
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
                // `unit.port$ = value` — simple_port write.
                if let Some(port) = name.strip_suffix('$') {
                    let port = port.to_owned();
                    let object_value = self.eval(object, frame, me)?;
                    let Value::Object(index) = object_value else {
                        return Err(self.error(
                            "E0402",
                            "port assignment requires an object",
                            span,
                        ));
                    };
                    return self.write_port_value(&port, value, Some(index), span);
                }
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

    /// `port$` read through the simulator bridge.
    fn read_port_value(
        &mut self,
        port: &str,
        me: Option<usize>,
        span: crate::ast::Span,
    ) -> Result<Value, Diagnostic> {
        let Some(index) = me else {
            return Err(self.error("E0402", "port access requires an enclosing instance", span));
        };
        let Some(path) = self.instances[index].port_path(port).map(str::to_owned) else {
            return Err(self.error("E0401", format!("unknown port '{port}'"), span));
        };
        if self.hdl.is_none() {
            return Err(self.error("E0408", "no simulator loaded — call hdl_load first", span));
        }
        let session = self.hdl.as_mut().unwrap();
        match session.get(&path) {
            Ok(value) => Ok(Value::Int(value)),
            Err(msg) => Err(self.error("E0408", msg, span)),
        }
    }

    /// `port$ = value` write through the simulator bridge.
    fn write_port_value(
        &mut self,
        port: &str,
        value: Value,
        me: Option<usize>,
        span: crate::ast::Span,
    ) -> Result<(), Diagnostic> {
        let Some(index) = me else {
            return Err(self.error("E0402", "port access requires an enclosing instance", span));
        };
        let Some(path) = self.instances[index].port_path(port).map(str::to_owned) else {
            return Err(self.error("E0401", format!("unknown port '{port}'"), span));
        };
        let Some(int_value) = as_i64(&value) else {
            return Err(self.error(
                "E0402",
                "port assignment expects an integer (or boolean) value",
                span,
            ));
        };
        if self.hdl.is_none() {
            return Err(self.error("E0408", "no simulator loaded — call hdl_load first", span));
        }
        let session = self.hdl.as_mut().unwrap();
        match session.set(&path, int_value) {
            Ok(()) => Ok(()),
            Err(msg) => Err(self.error("E0408", msg, span)),
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

/// Integer view of a value for the `hdl_*` routines: integers pass
/// through, booleans become 0/1 (handy for clock and reset pins).
/// Stable type key for an inline enumeral list.
fn inline_enum_key(names: &[String]) -> String {
    format!("[{}]", names.join(","))
}

fn as_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Int(i) => i64::try_from(*i).ok(),
        Value::Bool(b) => Some(i64::from(*b)),
        _ => None,
    }
}

/// Whether the expression calls any of the named routines anywhere in its
/// tree (including as a member call like `x.hdl_path()`).
fn expr_calls_any(expr: &Expr, names: &[&str]) -> bool {
    match expr {
        Expr::Call { callee, args, .. } => {
            let callee_name = match callee.as_ref() {
                Expr::Ident { name, .. } => Some(name.as_str()),
                Expr::MemberAccess { name, .. } => Some(name.as_str()),
                _ => None,
            };
            callee_name.is_some_and(|name| names.contains(&name))
                || args.iter().any(|arg| expr_calls_any(arg, names))
        }
        Expr::MemberAccess { object, .. } => expr_calls_any(object, names),
        Expr::Unary { expr: inner, .. }
        | Expr::Paren { expr: inner, .. }
        | Expr::CastAs { expr: inner, .. } => expr_calls_any(inner, names),
        Expr::Binary { lhs, rhs, .. } => expr_calls_any(lhs, names) || expr_calls_any(rhs, names),
        Expr::Ternary {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_calls_any(cond, names)
                || expr_calls_any(then_expr, names)
                || expr_calls_any(else_expr, names)
        }
        Expr::BitSlice {
            base, high, low, ..
        } => {
            expr_calls_any(base, names) || expr_calls_any(high, names) || expr_calls_any(low, names)
        }
        Expr::Index { base, index, .. } => {
            expr_calls_any(base, names) || expr_calls_any(index, names)
        }
        Expr::InSet { element, set, .. } => {
            let set_has = match set {
                crate::ast::InSetExpr::Items(items) => {
                    items.iter().any(|item| expr_calls_any(item, names))
                }
                crate::ast::InSetExpr::Range(low, high) => {
                    expr_calls_any(low, names) || expr_calls_any(high, names)
                }
            };
            expr_calls_any(element, names) || set_has
        }
        _ => false,
    }
}

fn as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Int(i) => Some(*i as f64),
        Value::Real(r) => Some(*r),
        _ => None,
    }
}

/// The element type of a list type expression (identity for non-lists).
fn list_element_type(ty: &TypeExpr) -> TypeExpr {
    match ty {
        TypeExpr::List { element, .. } => (**element).clone(),
        other => other.clone(),
    }
}

/// The bit width of a scalar type declaration (`uint(bits: 3)` → 3);
/// unsized scalars count as one bit for packing.
fn field_width_bits(ty: &TypeExpr) -> Option<i128> {
    let TypeExpr::Scalar {
        width: Some(width), ..
    } = ty
    else {
        return None;
    };
    match width.as_ref() {
        Expr::IntLiteral { text, .. } => parse_int_literal(text),
        _ => None,
    }
}

/// Whether the value digits of a sized or unsized based literal include an
/// MVL letter (clause 4.1.4.3). The base character itself (`'h`, `'x`) is
/// not part of the value. In an ambiguous context those letters make the
/// literal MVL rather than a number; hex `x` is therefore not read as 15.
pub(crate) fn based_value_is_mvl(text: &str) -> bool {
    let Some(quote) = text.find('\'') else {
        return false;
    };
    text[quote + 1..].chars().skip(1).any(|c| {
        matches!(
            c,
            'x' | 'X' | 'z' | 'Z' | 'u' | 'U' | 'l' | 'L' | 'h' | 'H' | 'w' | 'W' | 'n' | 'N'
        )
    })
}

/// Parse an integer literal: decimal (optional `K`/`M` multiplier),
/// `0c"..."` character, `0x`/`0b`/`0o` prefixed, or a sized/unsized based
/// literal (`8'hFF`, `'hFF`). Underscores are ignored. The size prefix is
/// ignored (widths are not truncated yet). MVL literals return [`None`].
pub(crate) fn parse_int_literal(text: &str) -> Option<i128> {
    if text.len() >= 2
        && text.as_bytes()[0] == b'0'
        && text.as_bytes()[1].eq_ignore_ascii_case(&b'c')
    {
        return parse_character_literal(text);
    }
    if based_value_is_mvl(text) {
        return None;
    }
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
    let (body, factor) = decimal_multiplier(&cleaned);
    let value = body.parse::<i128>().ok()?;
    value.checked_mul(factor)
}

/// `K`/`k` is ×1024 and `M`/`m` is ×1024×1024, on a decimal integer only.
fn decimal_multiplier(text: &str) -> (&str, i128) {
    let Some(last) = text.chars().last() else {
        return (text, 1);
    };
    let factor = match last {
        'k' => 1024,
        'm' => 1024 * 1024,
        _ => return (text, 1),
    };
    let body = &text[..text.len() - last.len_utf8()];
    if body.is_empty() || !body.bytes().all(|byte| byte.is_ascii_digit()) {
        return (text, 1);
    }
    (body, factor)
}

/// `0c"..."` / `0C"..."` evaluates to the character's code point.
fn parse_character_literal(text: &str) -> Option<i128> {
    let rest = if let Some(rest) = text.strip_prefix("0c") {
        rest
    } else {
        text.strip_prefix("0C")?
    };
    let inner = rest.strip_prefix('"')?.strip_suffix('"')?;
    let mut chars = inner.chars();
    let value = match chars.next()? {
        '\\' => match chars.next()? {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            '\\' => '\\',
            '"' => '"',
            '0' => '\0',
            _ => return None,
        },
        other => other,
    };
    if chars.next().is_some() {
        return None;
    }
    Some(u32::from(value) as i128)
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
