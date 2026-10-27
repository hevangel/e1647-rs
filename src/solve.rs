//! Constraint IR and the pluggable solver seam behind `gen`.
//!
//! `translate` turns the top-level `keep` constraints of one type, over the
//! integer fields of that type, into a [`Problem`]: bounded-integer
//! variables plus a conjunction of linear comparisons. The translation is
//! deliberately conservative — anything outside the supported shape
//! (non-integer fields, method calls, `||`, `!`, non-linear arithmetic)
//! reports [`TranslationError::Unsupported`] and the caller falls back to
//! the evaluation-based rejection sampler in [`crate::interp`].
//!
//! Backends:
//! - **pumpkin** (default-off cargo feature `solver-pumpkin`): the
//!   Pumpkin lazy-clause-generation CP solver (pure Rust,
//!   <https://github.com/consol-lab/pumpkin>), seeded for deterministic
//!   runs. Solves constraint shapes rejection sampling cannot (e.g.
//!   `a + b == 100`).
//! - **fallback**: without the feature [`solve`] reports
//!   [`SolveOutcome::Unsupported`] and `gen` uses rejection sampling.
//!
//! Diagnostics live in [`crate::interp`] (`E0407` for unsatisfiable
//! constraints); this module reports outcomes, not diagnostics.

use crate::ast::Expr;
use std::collections::BTreeMap;

/// One bounded-integer variable of a [`Problem`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedInt {
    pub name: String,
    pub lower: i64,
    pub upper: i64,
}

/// Comparison operator of a linear atom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// `Σ (coefficient · variable)  (cmp)  rhs` over integer variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Linear {
    pub terms: Vec<(String, i64)>,
    pub rhs: i64,
    pub cmp: Cmp,
}

/// A conjunction of linear atoms (`||` and `!` are not translated).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoolNode {
    Atom(Linear),
    All(Vec<BoolNode>),
}

/// A translatable `gen` problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub vars: Vec<BoundedInt>,
    pub constraints: Vec<BoolNode>,
}

/// Why a set of `keep` constraints could not be translated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslationError {
    /// The construct is outside the solver IR; the caller should fall back
    /// to rejection sampling (which enforces the constraints by
    /// evaluation).
    Unsupported(&'static str),
}

/// Outcome of solving a [`Problem`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SolveOutcome {
    /// An assignment for every problem variable (same order as
    /// [`Problem::vars`]).
    Solved(Vec<i128>),
    /// The constraints admit no assignment.
    Unsat,
    /// The backend gave up or is not compiled in; fall back to rejection
    /// sampling.
    Unsupported,
}

/// Declared bounds of an integer scalar type, capped to the solver's i32
/// domain (`uint` defaults to 31 bits here; arithmetic is not truncated,
/// only the randomization domain is).
pub fn int_bounds(ty: &crate::ast::TypeExpr) -> Option<(i64, i64)> {
    use crate::ast::{ScalarBase, TypeExpr};
    let TypeExpr::Scalar { base, width, .. } = ty else {
        return None;
    };
    let width_literal = width.as_deref().and_then(|expr| match expr {
        Expr::IntLiteral { text, .. } => crate::interp::parse_int_literal(text),
        _ => None,
    });
    let bits = width_literal.map(|v| v.clamp(1, 31) as u32);
    match base {
        ScalarBase::Uint => {
            let bits = bits.unwrap_or(31);
            Some((0, (1i64 << bits) - 1))
        }
        ScalarBase::Int => {
            let bits = bits.unwrap_or(31);
            Some((-(1i64 << (bits - 1)), (1i64 << (bits - 1)) - 1))
        }
        ScalarBase::Bit => Some((0, 1)),
        ScalarBase::Byte => Some((0, 255)),
        ScalarBase::Nibble => Some((0, 15)),
        ScalarBase::Bool | ScalarBase::Real | ScalarBase::String | ScalarBase::Time => None,
    }
}

/// Translate top-level `keep` constraint expressions over the given
/// variable bounds into a [`Problem`]. The bounds double as the variable
/// universe: a constraint referencing an identifier outside `bounds` (an
/// enumeral field, a list, a method call, …) makes the translation
/// unsupported.
pub fn translate(
    constraints: &[Expr],
    bounds: &BTreeMap<String, (i64, i64)>,
) -> Result<Problem, TranslationError> {
    let mut vars: Vec<BoundedInt> = bounds
        .iter()
        .map(|(name, (lower, upper))| BoundedInt {
            name: name.clone(),
            lower: *lower,
            upper: *upper,
        })
        .collect();
    vars.sort_by(|a, b| a.name.cmp(&b.name));
    let mut constraints_nodes = Vec::new();
    for constraint in constraints {
        constraints_nodes.push(translate_bool(constraint, bounds)?);
    }
    Ok(Problem {
        vars,
        constraints: constraints_nodes,
    })
}

fn translate_bool(
    expr: &Expr,
    bounds: &BTreeMap<String, (i64, i64)>,
) -> Result<BoolNode, TranslationError> {
    if let Expr::Binary {
        op: crate::ast::BinaryOp::LogicalAnd,
        lhs,
        rhs,
        ..
    } = expr
    {
        // Nested conjunctions stay nested; posting walks the tree.
        return Ok(BoolNode::All(vec![
            translate_bool(lhs, bounds)?,
            translate_bool(rhs, bounds)?,
        ]));
    }
    translate_atom(expr, bounds).map(BoolNode::Atom)
}

fn translate_atom(
    expr: &Expr,
    bounds: &BTreeMap<String, (i64, i64)>,
) -> Result<Linear, TranslationError> {
    let Expr::Binary { op, lhs, rhs, .. } = expr else {
        return Err(TranslationError::Unsupported(
            "constraint is not a comparison",
        ));
    };
    let cmp = match op {
        crate::ast::BinaryOp::Eq | crate::ast::BinaryOp::IdentityEq => Cmp::Eq,
        crate::ast::BinaryOp::NotEq | crate::ast::BinaryOp::IdentityNe => Cmp::Ne,
        crate::ast::BinaryOp::Lt => Cmp::Lt,
        crate::ast::BinaryOp::Le => Cmp::Le,
        crate::ast::BinaryOp::Gt => Cmp::Gt,
        crate::ast::BinaryOp::Ge => Cmp::Ge,
        _ => {
            return Err(TranslationError::Unsupported(
                "constraint operator is not a comparison",
            ));
        }
    };
    let left = affine_of(lhs, bounds)?;
    let right = affine_of(rhs, bounds)?;
    // l (cmp) r  ⟺  Σ (l.terms - r.terms) (cmp) r.constant - l.constant
    let mut merged: BTreeMap<String, i64> = BTreeMap::new();
    for (name, coefficient) in left.terms {
        *merged.entry(name).or_insert(0) += coefficient;
    }
    for (name, coefficient) in right.terms {
        *merged.entry(name).or_insert(0) -= coefficient;
    }
    let terms: Vec<(String, i64)> = merged
        .into_iter()
        .filter(|(_, coefficient)| *coefficient != 0)
        .collect();
    Ok(Linear {
        terms,
        rhs: right.constant - left.constant,
        cmp,
    })
}

/// An affine expression `Σ (coefficient · variable) + constant` over the
/// problem variables.
#[derive(Debug, Clone)]
struct Affine {
    terms: BTreeMap<String, i64>,
    constant: i64,
}

impl Affine {
    fn constant(value: i64) -> Self {
        Self {
            terms: BTreeMap::new(),
            constant: value,
        }
    }

    fn variable(name: String) -> Self {
        let mut terms = BTreeMap::new();
        terms.insert(name, 1);
        Self { terms, constant: 0 }
    }

    fn add(mut self, other: &Affine) -> Self {
        for (name, coefficient) in &other.terms {
            *self.terms.entry(name.clone()).or_insert(0) += coefficient;
        }
        self.constant += other.constant;
        self
    }

    fn subtract(mut self, other: &Affine) -> Self {
        for (name, coefficient) in &other.terms {
            *self.terms.entry(name.clone()).or_insert(0) -= coefficient;
        }
        self.constant -= other.constant;
        self
    }

    fn scale(self, factor: i64) -> Self {
        Self {
            terms: self
                .terms
                .into_iter()
                .map(|(name, coefficient)| (name, coefficient * factor))
                .collect(),
            constant: self.constant * factor,
        }
    }
}

fn affine_of(
    expr: &Expr,
    bounds: &BTreeMap<String, (i64, i64)>,
) -> Result<Affine, TranslationError> {
    match expr {
        Expr::IntLiteral { text, .. } => {
            let value = crate::interp::parse_int_literal(text)
                .and_then(|v| i64::try_from(v).ok())
                .ok_or(TranslationError::Unsupported(
                    "integer literal out of range",
                ))?;
            Ok(Affine::constant(value))
        }
        Expr::BitLiteral { text, .. } => {
            let value = crate::interp::parse_int_literal(text)
                .and_then(|v| i64::try_from(v).ok())
                .ok_or(TranslationError::Unsupported("based literal out of range"))?;
            Ok(Affine::constant(value))
        }
        Expr::Ident { name, .. } => {
            if bounds.contains_key(name) {
                Ok(Affine::variable(name.clone()))
            } else {
                Err(TranslationError::Unsupported(
                    "constraint references a non-integer or unknown identifier",
                ))
            }
        }
        Expr::Paren { expr, .. } => affine_of(expr, bounds),
        Expr::Unary {
            op: crate::ast::UnaryOp::Negate,
            expr,
            ..
        } => Ok(affine_of(expr, bounds)?.scale(-1)),
        Expr::Unary {
            op: crate::ast::UnaryOp::Plus,
            expr,
            ..
        } => affine_of(expr, bounds),
        Expr::Binary {
            op: crate::ast::BinaryOp::Add,
            lhs,
            rhs,
            ..
        } => Ok(affine_of(lhs, bounds)?.add(&affine_of(rhs, bounds)?)),
        Expr::Binary {
            op: crate::ast::BinaryOp::Sub,
            lhs,
            rhs,
            ..
        } => Ok(affine_of(lhs, bounds)?.subtract(&affine_of(rhs, bounds)?)),
        Expr::Binary {
            op: crate::ast::BinaryOp::Mul,
            lhs,
            rhs,
            ..
        } => {
            let left = affine_of(lhs, bounds)?;
            let right = affine_of(rhs, bounds)?;
            // Only literal × affine stays linear.
            if left.terms.is_empty() {
                Ok(right.scale(left.constant))
            } else if right.terms.is_empty() {
                Ok(left.scale(right.constant))
            } else {
                Err(TranslationError::Unsupported(
                    "non-linear multiplication in constraint",
                ))
            }
        }
        _ => Err(TranslationError::Unsupported(
            "constraint contains a non-arithmetic expression",
        )),
    }
}

/// Solve a problem with the best available backend. With the
/// `solver-pumpkin` feature this is the Pumpkin CP solver (seeded,
/// deterministic); without it the outcome is always
/// [`SolveOutcome::Unsupported`] and the caller falls back to rejection
/// sampling.
pub fn solve(problem: &Problem, seed: u64) -> SolveOutcome {
    #[cfg(feature = "solver-pumpkin")]
    {
        solve_with_pumpkin(problem, seed)
    }
    #[cfg(not(feature = "solver-pumpkin"))]
    {
        let _ = (problem, seed);
        SolveOutcome::Unsupported
    }
}

#[cfg(feature = "solver-pumpkin")]
mod pumpkin_backend {
    use super::{BoolNode, Cmp, Problem, SolveOutcome};
    use pumpkin_conflict_resolvers::resolvers::ResolutionResolver;
    use pumpkin_solver::Solver;
    use pumpkin_solver::core::constraints::NegatableConstraint;
    use pumpkin_solver::core::options::SolverOptions;
    use pumpkin_solver::core::proof::ProofLog;
    use pumpkin_solver::core::rand::SeedableRng;
    use pumpkin_solver::core::rand::rngs::SmallRng;
    use pumpkin_solver::core::results::{ProblemSolution, SatisfactionResult};
    use pumpkin_solver::core::termination::Indefinite;
    use pumpkin_solver::core::variables::{DomainId, TransformableVariable};
    use std::collections::BTreeMap;

    /// Everything the backend works with fits comfortably in i32; wider
    /// domains fall back to rejection sampling.
    const I32_LIMIT: i64 = 1 << 30;

    pub(super) fn solve_with_pumpkin(problem: &Problem, seed: u64) -> SolveOutcome {
        let mut solver = Solver::with_options(SolverOptions {
            memory_preallocated: 50,
            restart_options: Default::default(),
            random_generator: SmallRng::seed_from_u64(seed),
            proof_log: ProofLog::default(),
            learning_options: Default::default(),
            analysis_mode: Default::default(),
        });
        let mut variables: BTreeMap<&str, DomainId> = BTreeMap::new();
        let mut bounds: BTreeMap<&str, (i64, i64)> = BTreeMap::new();
        for var in &problem.vars {
            if var.lower < -I32_LIMIT || var.upper > I32_LIMIT {
                return SolveOutcome::Unsupported;
            }
            variables.insert(
                var.name.as_str(),
                solver.new_bounded_integer(var.lower as i32, var.upper as i32),
            );
            bounds.insert(var.name.as_str(), (var.lower, var.upper));
        }
        for node in &problem.constraints {
            if post_node(&mut solver, &variables, &bounds, node).is_err() {
                return SolveOutcome::Unsupported;
            }
        }
        let mut termination = Indefinite;
        let mut brancher = solver.default_brancher();
        let mut resolver = ResolutionResolver::default();
        match solver.satisfy(&mut brancher, &mut termination, &mut resolver) {
            SatisfactionResult::Satisfiable(satisfiable) => {
                let solution = satisfiable.solution();
                let assignment = problem
                    .vars
                    .iter()
                    .map(|var| i128::from(solution.get_integer_value(variables[var.name.as_str()])))
                    .collect();
                SolveOutcome::Solved(assignment)
            }
            SatisfactionResult::Unsatisfiable(..) => SolveOutcome::Unsat,
            SatisfactionResult::Unknown(..) => SolveOutcome::Unsupported,
        }
    }

    /// Post one conjunction node. Returns `Err` on a shape the backend does
    /// not handle (the caller falls back to rejection sampling).
    fn post_node(
        solver: &mut Solver,
        variables: &BTreeMap<&str, DomainId>,
        bounds: &BTreeMap<&str, (i64, i64)>,
        node: &BoolNode,
    ) -> Result<(), ()> {
        match node {
            BoolNode::All(children) => {
                for child in children {
                    post_node(solver, variables, bounds, child)?;
                }
                Ok(())
            }
            BoolNode::Atom(linear) => post_atom(solver, variables, bounds, linear),
        }
    }

    fn post_atom(
        solver: &mut Solver,
        variables: &BTreeMap<&str, DomainId>,
        bounds: &BTreeMap<&str, (i64, i64)>,
        linear: &super::Linear,
    ) -> Result<(), ()> {
        // Constant-only atoms are decided without the solver.
        if linear.terms.is_empty() {
            let holds = match linear.cmp {
                Cmp::Eq => 0 == linear.rhs,
                Cmp::Ne => 0 != linear.rhs,
                Cmp::Lt => 0 < linear.rhs,
                Cmp::Le => 0 <= linear.rhs,
                Cmp::Gt => 0 > linear.rhs,
                Cmp::Ge => 0 >= linear.rhs,
            };
            return if holds { Ok(()) } else { Err(()) };
        }
        if linear.rhs.abs() > I32_LIMIT {
            return Err(());
        }
        // The sum of scaled terms must stay inside the solver's i32 domain.
        let mut lower_sum: i64 = 0;
        let mut upper_sum: i64 = 0;
        for (name, coefficient) in &linear.terms {
            if coefficient.abs() > I32_LIMIT {
                return Err(());
            }
            let Some(&(lo, hi)) = bounds.get(name.as_str()) else {
                return Err(());
            };
            let (a, b) = (
                coefficient.saturating_mul(lo),
                coefficient.saturating_mul(hi),
            );
            lower_sum += a.min(b);
            upper_sum += a.max(b);
            if lower_sum < -I32_LIMIT || upper_sum > I32_LIMIT {
                return Err(());
            }
        }
        let tag = solver.new_constraint_tag();
        let scaled_terms: Vec<_> = linear
            .terms
            .iter()
            .map(|(name, coefficient)| variables[name.as_str()].scaled(*coefficient as i32))
            .collect();
        // Each builder returns its own opaque `impl NegatableConstraint`,
        // so every arm posts directly instead of producing one value.
        match linear.cmp {
            Cmp::Eq => {
                solver
                    .add_constraint(pumpkin_constraints::equals(
                        scaled_terms,
                        linear.rhs as i32,
                        tag,
                    ))
                    .post();
            }
            Cmp::Ne => {
                solver
                    .add_constraint(
                        pumpkin_constraints::equals(scaled_terms, linear.rhs as i32, tag)
                            .negation(),
                    )
                    .post();
            }
            Cmp::Le => {
                solver
                    .add_constraint(pumpkin_constraints::less_than_or_equals(
                        scaled_terms,
                        linear.rhs as i32,
                        tag,
                    ))
                    .post();
            }
            Cmp::Lt => {
                solver
                    .add_constraint(pumpkin_constraints::less_than_or_equals(
                        scaled_terms,
                        (linear.rhs - 1) as i32,
                        tag,
                    ))
                    .post();
            }
            // Σ c·x ≥ r  ⟺  Σ (-c)·x ≤ -r  (strictly: ≤ -r-1 for Gt).
            Cmp::Ge => {
                solver
                    .add_constraint(pumpkin_constraints::less_than_or_equals(
                        scaled_terms
                            .into_iter()
                            .map(|t| t.scaled(-1))
                            .collect::<Vec<_>>(),
                        (-linear.rhs) as i32,
                        tag,
                    ))
                    .post();
            }
            Cmp::Gt => {
                solver
                    .add_constraint(pumpkin_constraints::less_than_or_equals(
                        scaled_terms
                            .into_iter()
                            .map(|t| t.scaled(-1))
                            .collect::<Vec<_>>(),
                        (-(linear.rhs + 1)) as i32,
                        tag,
                    ))
                    .post();
            }
        }
        Ok(())
    }
}

#[cfg(feature = "solver-pumpkin")]
use pumpkin_backend::solve_with_pumpkin;
