//! The temporal-engine core (clause 12): sampling propagation
//! (12.1.4) and tight-satisfaction evaluation over finite paths
//! (12.1.1.2, 12.3) for the parsed [`TemporalExpr`] trees.
//!
//! This module is a pure core the interpreter will drive: a path is an
//! explicit sequence of states (each state = the events that occurred in
//! that sampling period), expressions are evaluated through caller
//! supplied callbacks, and nothing here touches simulation time or the
//! run loop — the `wait`/event-scheduler integration is a later
//! milestone.
//!
//! Coverage of the grammar:
//! - evaluated: event atoms, `cycle`, `true(exp)` (via the callbacks),
//!   sequences, `=>` yield, `and`/`or`/`not`/`fail`/`eventually`,
//!   fixed and true-match repeats, sampling and grouping;
//! - not evaluated: `change/rise/fall(exp)` edge samples (they need a
//!   sampled value history — [`EvalError::EdgeSample`]) and
//!   first-match repeats outside sequences ([`EvalError::Unsupported`]);
//!   `detach`/`delay`/`exec` never parse (see the parser's `E0120`).
//!
//! Semantics are documented per function against their subclauses.

use crate::ast::{EdgeRoutine, RepeatKind, TemporalExpr};
use std::collections::BTreeSet;

/// One state of a path (12.1.1.9): which events occurred in this
/// sampling period. Event paths are dotted strings (`"sys.any"`,
/// `"clk"`), matching [`TemporalExpr::EventRef`] paths.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    occurred: BTreeSet<String>,
}

impl State {
    pub fn new(occurred: &[&str]) -> Self {
        Self {
            occurred: occurred.iter().map(|event| (*event).to_owned()).collect(),
        }
    }

    pub fn occurs(&self, path: &[String]) -> bool {
        let dotted = path.join(".");
        self.occurred.contains(&dotted)
    }

    /// Record an event occurrence in this state (the interpreter's
    /// `emit` feeds the run log through this).
    pub fn record(&mut self, path: &str) {
        self.occurred.insert(path.to_owned());
    }
}

/// `true(exp)` propositions: `Some(TRUE/FALSE)` when evaluated.
pub type BooleanEval<'a> = Box<dyn Fn(&crate::ast::Expr) -> Option<bool> + 'a>;
/// Numeric expressions (repeat bounds): `Some(value)` when evaluated.
pub type NumericEval<'a> = Box<dyn Fn(&crate::ast::Expr) -> Option<i128> + 'a>;

/// The expression-evaluation callbacks of one evaluation: `true(exp)`
/// propositions and numeric repeat bounds both need the interpreter,
/// which is not wired into this pure core yet.
pub struct EvalCallbacks<'a> {
    /// `true(exp)` propositions: `Some(TRUE/FALSE)` when evaluated.
    pub boolean: BooleanEval<'a>,
    /// Numeric expressions (repeat bounds): `Some(value)` when
    /// evaluated.
    pub numeric: NumericEval<'a>,
}

/// Why a temporal expression could not be evaluated on a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalError {
    /// `change/rise/fall(exp)` needs the sampled value history of its
    /// expression across the path's states.
    EdgeSample(&'static str),
    /// A construct the evaluator does not handle (`detach`/`delay`/
    /// `exec` never parse; first-match repeats are handled inside
    /// sequences only) or an expression the callbacks could not
    /// evaluate.
    Unsupported(&'static str),
}

/// Sampling propagation (12.1.4): return the sampled normal form
/// `S(t, q)` — every temporal atom under `t` carries the sampling event
/// `q` unless it carries its own. Per 12.2.5 the sampling event
/// "applies to all sub-expressions" and can be "overridden for a
/// sub-expression by attaching a different sampling event". The clause's
/// worked example `{@a; ~[..]*cycle; @b} @q` becomes
/// `{@a @q; ~[..]*cycle @q; @b @q} @q`: each direct sub-expression
/// carries the event attached to it as a unit, the braces themselves
/// keep `@q`, and the interior of a repeat is left alone (`cycle`
/// already denotes one cycle of the context event, 12.2.2).
pub fn sampled_normal_form(te: &TemporalExpr, sampling: &TemporalExpr) -> TemporalExpr {
    normal_form(te, sampling)
}

/// The sampled normal form of `te` whose own sampling event is `q`.
fn normal_form(te: &TemporalExpr, q: &TemporalExpr) -> TemporalExpr {
    let sampled = |inner: TemporalExpr| crate::ast::TemporalExpr::Sampled {
        te: Box::new(inner),
        event: Box::new(q.clone()),
    };
    let sub = |child: &TemporalExpr| sub_form(child, q);
    match te {
        // Temporal atoms (12.1.1.13): the sampling event attaches to
        // the atom.
        crate::ast::TemporalExpr::EventRef { .. } => sampled(te.clone()),
        crate::ast::TemporalExpr::Cycle => sampled(te.clone()),
        crate::ast::TemporalExpr::True { expr } => {
            sampled(crate::ast::TemporalExpr::True { expr: expr.clone() })
        }
        crate::ast::TemporalExpr::EdgeSample { routine, expr } => {
            sampled(crate::ast::TemporalExpr::EdgeSample {
                routine: *routine,
                expr: expr.clone(),
            })
        }
        // The sequence keeps the sampling on the braces (12.1.4's
        // worked example) while each item carries it too (12.2.8: the
        // items are evaluated over the same sampling event's periods).
        crate::ast::TemporalExpr::Sequence { items } => {
            sampled(crate::ast::TemporalExpr::Sequence {
                items: items.iter().map(&sub).collect(),
            })
        }
        // The worked example shows the repeat sampled as a unit —
        // `~[..]*cycle @q` — with its interior untouched: `cycle`
        // already means one cycle of the context sampling event
        // (12.2.2).
        crate::ast::TemporalExpr::Repeat {
            kind,
            low,
            high,
            te: inner,
        } => sampled(crate::ast::TemporalExpr::Repeat {
            kind: *kind,
            low: low.clone(),
            high: high.clone(),
            te: Box::new((**inner).clone()),
        }),
        // The remaining compounds propagate into their operands (12.2.5:
        // the sampling event "applies to all sub-expressions"); 12.2.13
        // states the context event applies to both sides of a yield.
        crate::ast::TemporalExpr::Yield { lhs, rhs } => crate::ast::TemporalExpr::Yield {
            lhs: Box::new(sub(lhs)),
            rhs: Box::new(sub(rhs)),
        },
        crate::ast::TemporalExpr::And(a, b) => {
            crate::ast::TemporalExpr::And(Box::new(sub(a)), Box::new(sub(b)))
        }
        crate::ast::TemporalExpr::Or(a, b) => {
            crate::ast::TemporalExpr::Or(Box::new(sub(a)), Box::new(sub(b)))
        }
        crate::ast::TemporalExpr::Not(inner) => crate::ast::TemporalExpr::Not(Box::new(sub(inner))),
        crate::ast::TemporalExpr::Fail(inner) => {
            crate::ast::TemporalExpr::Fail(Box::new(sub(inner)))
        }
        crate::ast::TemporalExpr::Eventually(inner) => {
            crate::ast::TemporalExpr::Eventually(Box::new(sub(inner)))
        }
        // An explicit sub-sampling overrides the context for everything
        // below it (12.2.5); the normal form is rebuilt under its own
        // event, whose wrapper is the result — no extra sampling layer.
        crate::ast::TemporalExpr::Sampled { te: inner, event } => normal_form(inner, event),
        // Groups are transparent to sampling.
        crate::ast::TemporalExpr::Group(inner) => normal_form(inner, q),
    }
}

/// A direct sub-expression of a compound: one that carries its own
/// sampling event keeps it (12.2.5's override — the context does not
/// descend past it), any other carries the context event.
fn sub_form(child: &TemporalExpr, q: &TemporalExpr) -> TemporalExpr {
    match child {
        crate::ast::TemporalExpr::Sampled { .. } => child.clone(),
        _ => normal_form(child, q),
    }
}

/// Whether `te` succeeds at the last point of `path` (12.1.1.12): there
/// is a sub-path ending at that point over which the expression holds
/// tightly. Propositions (`true(exp)`) and numeric repeat bounds are
/// evaluated through the callbacks; `None` from a callback means the
/// expression could not be evaluated.
///
/// The path is a sequence of consecutive sampling states of the
/// expression's sampling event (in sampled normal form every atom
/// shares it, so evaluation walks the same event's points).
pub fn succeeds(
    te: &TemporalExpr,
    path: &[State],
    callbacks: &EvalCallbacks,
) -> Result<bool, EvalError> {
    if path.is_empty() {
        return Ok(false);
    }
    // Success at the last point: some suffix holds tightly.
    for start in 0..path.len() {
        if holds_tight(te, &path[start..], callbacks)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Tight satisfaction (12.1.1.2) of `te` over the whole path: "a TE
/// holds tightly on a path iff the path exhibits the behavior described
/// by the TE" — every state of the path must be accounted for. This is
/// deliberately distinct from [`succeeds`] (12.1.1.12), which only asks
/// for a sub-path ending at the last point; 12.2's preamble illustrates
/// the difference with `cycle @q`, which holds tightly only on the
/// single-state path where `q` occurs.
pub fn holds_tightly(
    te: &TemporalExpr,
    path: &[State],
    callbacks: &EvalCallbacks,
) -> Result<bool, EvalError> {
    holds_tight(te, path, callbacks)
}

/// Tight satisfaction (12.1.1.2) of `te` over the whole state slice.
fn holds_tight(
    te: &TemporalExpr,
    states: &[State],
    callbacks: &EvalCallbacks,
) -> Result<bool, EvalError> {
    if states.is_empty() {
        return Ok(false);
    }
    match te {
        // An event atom holds on a single-state path where the event
        // occurs (success is simultaneous with the occurrence,
        // 12.2.4).
        crate::ast::TemporalExpr::EventRef { path } => {
            Ok(states.len() == 1 && states[0].occurs(path))
        }
        // One cycle of the sampling event: any single sampling state
        // (12.2.2).
        crate::ast::TemporalExpr::Cycle => Ok(states.len() == 1),
        // A proposition holds at the state where it is evaluated
        // (12.2.3).
        crate::ast::TemporalExpr::True { expr } => {
            Ok(states.len() == 1 && (callbacks.boolean)(expr) == Some(true))
        }
        // Edge samples need the value history across states.
        crate::ast::TemporalExpr::EdgeSample { routine, .. } => {
            Err(EvalError::EdgeSample(edge_routine_name(*routine)))
        }
        // A sequence holds when its items hold over consecutive,
        // non-overlapping segments, each item starting in the sampling
        // period following the one where the previous succeeded
        // (12.2.8). A first-match repeat item (12.2.11) tries every
        // repetition count within its bounds.
        crate::ast::TemporalExpr::Sequence { items } => seq_holds(items, states, callbacks),
        // Both TEs succeed in the same sampling periods (12.2.6).
        crate::ast::TemporalExpr::And(a, b) => {
            Ok(holds_tight(a, states, callbacks)? && holds_tight(b, states, callbacks)?)
        }
        // Either TE succeeds (12.2.7).
        crate::ast::TemporalExpr::Or(a, b) => {
            Ok(holds_tight(a, states, callbacks)? || holds_tight(b, states, callbacks)?)
        }
        // `not te` holds when the sub-expression does not hold tightly
        // on the slice (12.2.16).
        crate::ast::TemporalExpr::Not(inner) => Ok(!holds_tight(inner, states, callbacks)?),
        // `fail te` holds when every possibility to satisfy `te` is
        // exhausted on the slice (12.2.12).
        crate::ast::TemporalExpr::Fail(inner) => Ok(!succeeds(inner, states, callbacks)?),
        // `eventually te` holds when the sub-expression succeeds at any
        // point of the slice (12.2.14).
        crate::ast::TemporalExpr::Eventually(inner) => succeeds(inner, states, callbacks),
        // The yield (12.2.13): "succeeds without evaluating the second
        // expression if the first expression fails. If the first
        // expression succeeds, then the second expression needs to also
        // succeed in sequence." The lhs's successes within the slice
        // are its tight satisfactions on prefixes; each such success
        // obligates the rhs to succeed on the states after that prefix
        // (12.2.8: the following TE starts in the sampling period after
        // the preceding one succeeded). When the lhs cannot succeed
        // anywhere in the slice, the yield holds vacuously.
        crate::ast::TemporalExpr::Yield { lhs, rhs } => {
            // Every way the lhs holds tightly on a prefix of the slice
            // obligates the rhs to succeed on the states after that
            // prefix (12.2.8: the following TE starts in the sampling
            // period after the preceding one succeeded). A violated
            // obligation fails the yield; with no lhs success anywhere
            // in the slice there is no obligation and the yield holds
            // vacuously (12.2.13).
            for split in 1..=states.len() {
                if holds_tight(lhs, &states[..split], callbacks)?
                    && !succeeds(rhs, &states[split..], callbacks)?
                {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        crate::ast::TemporalExpr::Repeat {
            kind,
            low,
            high,
            te: inner,
        } => match kind {
            // `[n]*te` — exactly n consecutive occurrences (12.2.9):
            // the unrolled sequence holds.
            RepeatKind::Fixed => {
                let n = repeat_bound(low.as_ref(), callbacks)?;
                let unrolled = unroll(inner, n);
                holds_tight(&unrolled, states, callbacks)
            }
            // `~[l..h]*te` — a true-match repeat holds when the
            // sub-expression holds over k consecutive states for some
            // k in the bounds (12.2.10); `~[..]` is the unbounded-low
            // form, so the range starts at zero.
            RepeatKind::TrueMatch => {
                let low = match low.as_ref() {
                    Some(expr) => repeat_bound(Some(expr), callbacks)?,
                    None => 0,
                };
                let high = match high {
                    Some(expr) => repeat_bound(Some(expr), callbacks)?,
                    None => states.len(),
                };
                for k in low..=high.min(states.len()) {
                    let unrolled = unroll(inner, k);
                    if holds_tight(&unrolled, states, callbacks)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            // First-match repeats are sequence items (12.2.11) and are
            // handled by `seq_holds`.
            RepeatKind::FirstMatch => Err(EvalError::Unsupported(
                "first-match repeats are sequence constructs",
            )),
        },
        // In sampled normal form every atom carries the same sampling
        // event, so a Sampled node evaluates its inner over the same
        // states; the node is recorded for the engine's bookkeeping.
        crate::ast::TemporalExpr::Sampled { te: inner, .. } => {
            holds_tight(inner, states, callbacks)
        }
        crate::ast::TemporalExpr::Group(inner) => holds_tight(inner, states, callbacks),
    }
}

/// Sequence matching with strict splits and first-match repeats.
fn seq_holds(
    items: &[TemporalExpr],
    states: &[State],
    callbacks: &EvalCallbacks,
) -> Result<bool, EvalError> {
    let Some((first, rest)) = items.split_first() else {
        return Ok(true);
    };
    // A first-match repeat expands to k consecutive copies of its TE
    // (k in the bounds) followed by the rest of the sequence —
    // 12.2.11's equivalence `{[..]*TE1; @ev_a}` is
    // `{@ev_a} or {[1]*TE1; @ev_a} or {[2]*TE1; @ev_a} ...`, so an
    // omitted lower bound starts at zero.
    if let crate::ast::TemporalExpr::Repeat {
        kind: RepeatKind::FirstMatch,
        low,
        high,
        te: inner,
    } = first
    {
        let low = match low.as_ref() {
            Some(expr) => repeat_bound(Some(expr), callbacks)?,
            None => 0,
        };
        let high = match high {
            Some(expr) => repeat_bound(Some(expr), callbacks)?,
            None => states.len(),
        };
        for k in low..=high.min(states.len()) {
            let expanded = unroll(inner, k);
            let mut expanded_items = vec![expanded];
            expanded_items.extend_from_slice(rest);
            if seq_holds(&expanded_items, states, callbacks)? {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    // The first item holds on some prefix; the rest must hold on the
    // strictly following states.
    for split in 1..=states.len() {
        if holds_tight(first, &states[..split], callbacks)?
            && seq_holds(rest, &states[split..], callbacks)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `te` repeated `n` times as a sequence (an empty repetition holds
/// trivially, per 12.2.9's "evaluates to zero — the TE succeeds
/// immediately").
fn unroll(te: &TemporalExpr, n: usize) -> TemporalExpr {
    if n == 0 {
        return crate::ast::TemporalExpr::Sequence { items: Vec::new() };
    }
    crate::ast::TemporalExpr::Sequence {
        items: std::iter::repeat_n(te.clone(), n).collect(),
    }
}

/// Evaluate a repeat bound; bounds are ordinary numeric expressions.
fn repeat_bound(
    expr: Option<&crate::ast::Expr>,
    callbacks: &EvalCallbacks,
) -> Result<usize, EvalError> {
    match expr {
        Some(expr) => match (callbacks.numeric)(expr) {
            Some(value) if value >= 0 => Ok(value as usize),
            Some(_) => Err(EvalError::Unsupported("repeat bounds must not be negative")),
            None => Err(EvalError::Unsupported(
                "repeat bounds must evaluate to numeric values",
            )),
        },
        None => Err(EvalError::Unsupported(
            "a repeat bound is required for evaluation",
        )),
    }
}

fn edge_routine_name(routine: EdgeRoutine) -> &'static str {
    match routine {
        EdgeRoutine::Change => "change",
        EdgeRoutine::Rise => "rise",
        EdgeRoutine::Fall => "fall",
    }
}

/// Whether `te` succeeded at any point of `states`: some sub-path
/// (12.3.3's legal starts — the path start, or just after any sampling
/// point; the run loop's single timeline samples every cycle) over
/// which the TE holds tightly (12.3.1 at any end point). This is the
/// wait- and rule-side notion of "happened at some time", as opposed
/// to [`succeeds`]'s success at the last point only.
pub fn succeeded_somewhere(
    te: &TemporalExpr,
    states: &[State],
    callbacks: &EvalCallbacks,
) -> Result<bool, EvalError> {
    for end in 1..=states.len() {
        for start in 0..end {
            if holds_tight(te, &states[start..end], callbacks)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Whether an `expect`/`assume` rule (11.4) failed over a finished run
/// log — 12.3.2: the rule fails where `fail te` succeeds, i.e. where
/// all possibilities to satisfy the TE are exhausted. The per-shape
/// readings, over the whole log:
///
/// - a yield (the common `@a => eventually @b` shape) fails when the
///   lhs succeeded somewhere (12.3.3 legal starts) and the rhs never
///   succeeded after that (12.2.13: "needs to also succeed in
///   sequence");
/// - `eventually te` fails when the sub-expression never succeeded
///   (all possibilities exhausted at quit, 12.2.14);
/// - `not te` / `fail te` fail when the sub-expression did hold —
///   `not @a` as a rule is the invariant "a never occurs";
/// - every other top-level shape is success-required: it fails when it
///   never succeeded anywhere.
///
/// Approximations (documented, per the run loop's model): one global
/// timeline of per-cycle states (the sampling event's occurrences are
/// every cycle boundary), and propositions/bounds evaluated with the
/// values the callbacks supply — the interpreter has no per-cycle
/// value history. Edge samples propagate [`EvalError::EdgeSample`].
pub fn rule_failed(
    te: &TemporalExpr,
    log: &[State],
    callbacks: &EvalCallbacks,
) -> Result<bool, EvalError> {
    if log.is_empty() {
        return Ok(false);
    }
    match te {
        // An obligation pair: violated when some lhs success left no
        // rhs success after it. Enumerate the lhs's tight satisfactions
        // over legal starts and check each one's rhs window.
        crate::ast::TemporalExpr::Yield { lhs, rhs } => {
            for end in 1..=log.len() {
                for start in 0..end {
                    if holds_tight(lhs, &log[start..end], callbacks)?
                        && !succeeded_somewhere(rhs, &log[end..], callbacks)?
                    {
                        return Ok(true);
                    }
                }
            }
            Ok(false)
        }
        // The expected event never became possible-and-actual: failed
        // at quit.
        crate::ast::TemporalExpr::Eventually(inner) => {
            Ok(!succeeded_somewhere(inner, log, callbacks)?)
        }
        // `not te` / `fail te` rules are invariants: they fail when the
        // inner behavior did happen (12.2.16 vs 12.2.12, read over the
        // whole window).
        crate::ast::TemporalExpr::Not(inner) | crate::ast::TemporalExpr::Fail(inner) => {
            succeeded_somewhere(inner, log, callbacks)
        }
        // Success-required shapes: the rule must have succeeded at
        // least once before quit.
        _ => Ok(!succeeded_somewhere(te, log, callbacks)?),
    }
}

/// Collect the proposition (`true(exp)`) and numeric repeat-bound
/// expressions appearing in `te`, in evaluation order — the caller
/// evaluates them once (the values the callbacks will supply for every
/// state) and builds a callback set over the results.
pub fn collect_exprs<'te>(
    te: &'te TemporalExpr,
    propositions: &mut Vec<&'te crate::ast::Expr>,
    bounds: &mut Vec<&'te crate::ast::Expr>,
) {
    match te {
        crate::ast::TemporalExpr::True { expr } => propositions.push(expr),
        crate::ast::TemporalExpr::Repeat {
            low,
            high,
            te: inner,
            ..
        } => {
            if let Some(expr) = low {
                bounds.push(expr);
            }
            if let Some(expr) = high {
                bounds.push(expr);
            }
            collect_exprs(inner, propositions, bounds);
        }
        crate::ast::TemporalExpr::Sequence { items } => {
            for item in items {
                collect_exprs(item, propositions, bounds);
            }
        }
        crate::ast::TemporalExpr::Yield { lhs, rhs }
        | crate::ast::TemporalExpr::And(lhs, rhs)
        | crate::ast::TemporalExpr::Or(lhs, rhs) => {
            collect_exprs(lhs, propositions, bounds);
            collect_exprs(rhs, propositions, bounds);
        }
        crate::ast::TemporalExpr::Not(inner)
        | crate::ast::TemporalExpr::Fail(inner)
        | crate::ast::TemporalExpr::Eventually(inner)
        | crate::ast::TemporalExpr::Sampled { te: inner, .. }
        | crate::ast::TemporalExpr::Group(inner) => {
            collect_exprs(inner, propositions, bounds);
        }
        // Edge samples need the value history the callbacks cannot
        // supply; evaluation reports EvalError::EdgeSample before any
        // expression is consulted.
        crate::ast::TemporalExpr::EventRef { .. }
        | crate::ast::TemporalExpr::Cycle
        | crate::ast::TemporalExpr::EdgeSample { .. } => {}
    }
}
