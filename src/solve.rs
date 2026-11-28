//! Constraint IR and the pluggable solver seam behind `gen`.
//!
//! `translate` turns the top-level `keep` constraints of one type, over the
//! integer fields of that type, into a [`Problem`]: bounded-integer
//! variables plus a boolean tree of comparisons — conjunction
//! ([`BoolNode::All`]), disjunction ([`BoolNode::Any`]) and their negation
//! (`not` flips every comparison and swaps the connective, De Morgan).
//! Range membership (`x in [lo..hi]`) translates to the two comparisons;
//! item-list membership (`x in [a, b]`) translates to the disjunction of
//! its equalities. The atoms are linear ([`BoolNode::Atom`]) or — since
//! multiplication of two non-constant sides no longer gives up — a
//! polynomial ([`BoolNode::Poly`]): the product expands into monomials
//! (total degree capped at 4).
//! Bounded integer quotient comparisons with nonzero literal divisors
//! translate to inclusive ranges. Remainder equalities translate to
//! bounded disjunctions of matching values or a single congruence.
//!
//! Both built-in backends implement [`SolverBackend`]. [`Solver::Enumerate`]
//! searches bounded integer assignments in seeded order, including polynomial
//! constraints. [`Solver::Pumpkin`] solves linear and supported remainder
//! problems with bounded disjunction expansion and requires `solver-pumpkin`.
//! The default [`solve`] uses enumeration for polynomial problems and Pumpkin
//! for other problems. Unsupported problems defer to rejection sampling.
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

/// `Σ (coefficient · monomial)  (cmp)  rhs` — a polynomial comparison
/// over the problem variables; each monomial maps variables to positive
/// exponents. A monomial with one variable at exponent 1 is linear, so
/// `a * 2 == 10` stays a [`BoolNode::Atom`]; `a * b <= 100` and
/// `a * a == 2500` arrive here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Polynomial {
    pub terms: Vec<(BTreeMap<String, u32>, i64)>,
    pub rhs: i64,
    pub cmp: Cmp,
}

/// A boolean tree of comparison atoms. The backends post `Atom` and
/// `All` directly; `Any` is expanded by [`solve`] into branch
/// combinations before posting, and any [`BoolNode::Poly`] routes the
/// whole problem to the bounded enumeration solver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoolNode {
    Atom(Linear),
    Poly(Polynomial),
    /// Compare a bounded field's remainder to a fixed value.
    Remainder {
        field: String,
        divisor: i64,
        remainder: i64,
        cmp: Cmp,
    },
    All(Vec<BoolNode>),
    Any(Vec<BoolNode>),
}

impl BoolNode {
    /// Whether the tree contains a polynomial atom.
    fn has_polynomial(&self) -> bool {
        match self {
            BoolNode::Poly(_) => true,
            BoolNode::Atom(_) | BoolNode::Remainder { .. } => false,
            BoolNode::All(children) | BoolNode::Any(children) => {
                children.iter().any(BoolNode::has_polynomial)
            }
        }
    }
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

/// A solver for translated bounded-integer problems. A `Solved` assignment
/// must satisfy the problem and contain one value per variable in input order.
/// `Unsupported` allows the interpreter to try its runtime sampler.
pub trait SolverBackend {
    fn solve(&self, problem: &Problem, seed: u64) -> SolveOutcome;
}

/// Built-in solver selection for native and library callers.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Solver {
    #[default]
    Auto,
    Enumerate,
    Pumpkin,
}

impl Solver {
    /// Whether this selection is available in the current build.
    pub fn is_available(self) -> bool {
        self != Self::Pumpkin || cfg!(feature = "solver-pumpkin")
    }
}

impl std::str::FromStr for Solver {
    type Err = &'static str;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "auto" => Ok(Self::Auto),
            "enumerate" => Ok(Self::Enumerate),
            "pumpkin" => Ok(Self::Pumpkin),
            _ => Err("expected auto, enumerate, or pumpkin"),
        }
    }
}

impl SolverBackend for Solver {
    fn solve(&self, problem: &Problem, seed: u64) -> SolveOutcome {
        let polynomial = problem.constraints.iter().any(BoolNode::has_polynomial);
        match self {
            Self::Enumerate => solve_by_enumeration(problem, seed),
            Self::Auto if polynomial => solve_by_enumeration(problem, seed),
            Self::Pumpkin if polynomial || !self.is_available() => SolveOutcome::Unsupported,
            Self::Auto | Self::Pumpkin => solve_with_pumpkin_branches(problem, seed),
        }
    }
}

/// Integer bounds for the solver. Unsized int/uint use the solver's
/// 31-bit default; explicitly wider fields use the runtime sampler.
pub fn int_bounds(ty: &crate::ast::TypeExpr) -> Option<(i64, i64)> {
    use crate::ast::{ScalarBase, TypeExpr};
    let TypeExpr::Scalar { base, width, .. } = ty else {
        return None;
    };
    let width_literal = width.as_deref().and_then(|expr| match expr {
        Expr::IntLiteral { text, .. } => crate::interp::parse_int_literal(text),
        _ => None,
    });
    let solver_width = || match width_literal {
        Some(width) if width > 31 => None,
        Some(width) => Some(width.max(1) as u32),
        None => Some(31),
    };
    match base {
        ScalarBase::Uint => {
            let bits = solver_width()?;
            Some((0, (1i64 << bits) - 1))
        }
        ScalarBase::Int => {
            let bits = solver_width()?;
            Some((-(1i64 << (bits - 1)), (1i64 << (bits - 1)) - 1))
        }
        // The long bases are 64-bit wide (Table 17, clause 5.2.3.1);
        // their full value ranges exceed the solver's i64 bounded-integer
        // domain (a `longuint` reaches 2^64-1). Excluding them here sends
        // `gen` to the rejection sampler, which draws over the complete
        // 64-bit range instead of a narrowed one.
        ScalarBase::Longint | ScalarBase::Longuint => None,
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
    // A parenthesized constraint shape (`keep (a < 1 || a > 2);`) is
    // transparent here; the paren group's inner shape decides.
    if let Expr::Paren { expr: inner, .. } = expr {
        return translate_bool(inner, bounds);
    }
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
    if let Expr::Binary {
        op: crate::ast::BinaryOp::LogicalOr,
        lhs,
        rhs,
        ..
    } = expr
    {
        return Ok(BoolNode::Any(vec![
            translate_bool(lhs, bounds)?,
            translate_bool(rhs, bounds)?,
        ]));
    }
    // `not`/`!` flips every comparison and swaps the connective (De
    // Morgan); a doubly negated shape collapses back through the flip
    // table. The `a not in [..]` parse wraps the membership in this
    // unary, so negated membership is handled here too.
    if let Expr::Unary {
        op: crate::ast::UnaryOp::Not,
        expr: inner,
        ..
    } = expr
    {
        return translate_bool(inner, bounds).and_then(negate_node);
    }
    // `x in [lo..hi]` is the conjunction `x >= lo && x <= hi`.
    if let Expr::InSet {
        element,
        set: crate::ast::InSetExpr::Range(low, high),
        ..
    } = expr
    {
        return Ok(BoolNode::All(vec![
            BoolNode::Atom(linear_comparison(Cmp::Ge, element, low, bounds)?),
            BoolNode::Atom(linear_comparison(Cmp::Le, element, high, bounds)?),
        ]));
    }
    // `x in [a, b, c]` is the disjunction `x == a || x == b || x == c`;
    // each item goes through the same affine translation as a comparison
    // side, so affine item expressions translate too.
    if let Expr::InSet {
        element,
        set: crate::ast::InSetExpr::Items(items),
        ..
    } = expr
    {
        let branches = items
            .iter()
            .map(|item| linear_comparison(Cmp::Eq, element, item, bounds).map(BoolNode::Atom))
            .collect::<Result<Vec<_>, TranslationError>>()?;
        return Ok(BoolNode::Any(branches));
    }
    translate_comparison(expr, bounds)
}

/// One comparison atom: the affine translation first (it subsumes every
/// linear shape, which the backend posts directly); a comparison the
/// affine translation cannot express falls to the polynomial
/// translation, whose product expansion handles non-linear
/// multiplication. The atom error is the more specific diagnostic, so a
/// failed polynomial attempt reports it.
fn translate_comparison(
    expr: &Expr,
    bounds: &BTreeMap<String, (i64, i64)>,
) -> Result<BoolNode, TranslationError> {
    use crate::ast::BinaryOp;

    if let Expr::Binary { op, lhs, rhs, .. } = expr {
        let comparison = match op {
            BinaryOp::Eq => Some(Cmp::Eq),
            BinaryOp::NotEq => Some(Cmp::Ne),
            BinaryOp::Lt => Some(Cmp::Lt),
            BinaryOp::Le => Some(Cmp::Le),
            BinaryOp::Gt => Some(Cmp::Gt),
            BinaryOp::Ge => Some(Cmp::Ge),
            _ => None,
        };
        if let Some(direct) = comparison
            && let Some(node) = division_comparison(lhs, rhs, direct, bounds)
                .or_else(|| division_comparison(rhs, lhs, reverse_cmp(direct), bounds))
        {
            return if direct == Cmp::Ne {
                negate_node(node)
            } else {
                Ok(node)
            };
        }
    }
    match translate_atom(expr, bounds) {
        Ok(linear) => Ok(BoolNode::Atom(linear)),
        Err(atom_error) => match polynomial_comparison(expr, bounds) {
            Ok(polynomial) => Ok(BoolNode::Poly(polynomial)),
            Err(_) => Err(atom_error),
        },
    }
}

/// Restrict operands to signed 32-bit values so that the interpreter's
/// integer precision cannot alter them.
fn division_comparison(
    mut operation: &Expr,
    result: &Expr,
    mut cmp: Cmp,
    bounds: &BTreeMap<String, (i64, i64)>,
) -> Option<BoolNode> {
    use crate::ast::BinaryOp;

    while let Expr::Paren { expr, .. } = operation {
        operation = expr;
    }
    let Expr::Binary { op, lhs, rhs, .. } = operation else {
        return None;
    };
    if !matches!(op, BinaryOp::Div | BinaryOp::Mod) {
        return None;
    }
    let Expr::Ident { name, .. } = lhs.as_ref() else {
        return None;
    };
    let &(lower, upper) = bounds.get(name)?;
    let mut divisor = signed_decimal(rhs)?;
    let mut result = signed_decimal(result)?;
    let limit = i64::from(i32::MAX);
    if lower < i64::from(i32::MIN)
        || upper > limit
        || !(-limit..=limit).contains(&divisor)
        || divisor == 0
        || i32::try_from(result).is_err()
        || (lower >= 0 && (divisor < 0 || result < 0))
        || (*op == BinaryOp::Div && lower == i64::from(i32::MIN) && divisor == -1)
    {
        return None;
    }
    if divisor < 0 {
        divisor = -divisor;
        if *op == BinaryOp::Div {
            result = -result;
            cmp = reverse_cmp(cmp);
        }
    }
    let atom = |cmp, rhs| {
        BoolNode::Atom(Linear {
            terms: vec![(name.clone(), 1)],
            rhs,
            cmp,
        })
    };
    if *op == BinaryOp::Mod {
        if lower < 0 {
            let comparison = if cmp == Cmp::Ne { Cmp::Eq } else { cmp };
            let (low, high) = remainder_bounds(
                comparison,
                result,
                lower.max(1 - divisor),
                upper.max(0).min(divisor - 1),
            );
            if low > high {
                return Some(BoolNode::Any(Vec::new()));
            }
            return Some(BoolNode::Remainder {
                field: name.clone(),
                divisor,
                remainder: result,
                cmp: comparison,
            });
        }
        if !matches!(cmp, Cmp::Eq | Cmp::Ne) {
            let max_remainder = (divisor - 1).min(upper);
            let (low, high) = remainder_bounds(cmp, result, 0, max_remainder);
            if low > high {
                return Some(BoolNode::Any(Vec::new()));
            }
            if low == 0 && high == max_remainder {
                return Some(BoolNode::All(Vec::new()));
            }
            return Some(BoolNode::Remainder {
                field: name.clone(),
                divisor,
                remainder: result,
                cmp,
            });
        }
        if result >= divisor {
            return Some(BoolNode::Any(Vec::new()));
        }
        let first = lower + (result - lower).rem_euclid(divisor);
        if first > upper {
            return Some(BoolNode::Any(Vec::new()));
        }
        let count = (upper - first) / divisor + 1;
        if count > MAX_BRANCH_COMBINATIONS as i64 {
            return Some(BoolNode::Remainder {
                field: name.clone(),
                divisor,
                remainder: result,
                cmp: Cmp::Eq,
            });
        }
        return Some(BoolNode::Any(
            (0..count)
                .map(|offset| atom(Cmp::Eq, first + offset * divisor))
                .collect(),
        ));
    }
    // Truncation toward zero extends the preimage away from zero.
    // Quotient zero includes both sides of zero.
    let product = result * divisor;
    let start = if result <= 0 {
        product - divisor + 1
    } else {
        product
    };
    let end = if result >= 0 {
        product + divisor - 1
    } else {
        product
    };
    let (low, high) = match cmp {
        Cmp::Eq | Cmp::Ne => (start.max(lower), end.min(upper)),
        Cmp::Lt => (lower, (start - 1).min(upper)),
        Cmp::Le => (lower, end.min(upper)),
        Cmp::Gt => (lower.max(end + 1), upper),
        Cmp::Ge => (lower.max(start), upper),
    };
    if low > high || high < lower {
        return Some(BoolNode::Any(Vec::new()));
    }
    Some(BoolNode::All(vec![atom(Cmp::Ge, low), atom(Cmp::Le, high)]))
}

fn signed_decimal(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::IntLiteral { text, .. } => {
            i64::try_from(crate::interp::parse_int_literal(text)?).ok()
        }
        Expr::Paren { expr, .. } => signed_decimal(expr),
        Expr::Unary {
            op: crate::ast::UnaryOp::Negate,
            expr,
            ..
        } => signed_decimal(expr)?.checked_neg(),
        _ => None,
    }
}

fn reverse_cmp(cmp: Cmp) -> Cmp {
    match cmp {
        Cmp::Eq | Cmp::Ne => cmp,
        Cmp::Lt => Cmp::Gt,
        Cmp::Le => Cmp::Ge,
        Cmp::Gt => Cmp::Lt,
        Cmp::Ge => Cmp::Le,
    }
}

/// Restrict an ordered comparison to the possible residual interval.
fn remainder_bounds(cmp: Cmp, value: i64, minimum: i64, maximum: i64) -> (i64, i64) {
    let low = match cmp {
        Cmp::Eq | Cmp::Ge => minimum.max(value),
        Cmp::Gt => minimum.max(value + 1),
        _ => minimum,
    };
    let high = match cmp {
        Cmp::Eq | Cmp::Le => maximum.min(value),
        Cmp::Lt => maximum.min(value - 1),
        _ => maximum,
    };
    (low, high)
}

/// Negate one translated boolean tree: every comparison flips
/// (`Lt`↔`Ge`, `Le`↔`Gt`, `Eq`↔`Ne`) and `All`/`Any` swap (De Morgan).
fn negate_node(node: BoolNode) -> Result<BoolNode, TranslationError> {
    match node {
        BoolNode::Atom(linear) => Ok(BoolNode::Atom(Linear {
            cmp: flip_cmp(linear.cmp),
            ..linear
        })),
        BoolNode::Poly(polynomial) => Ok(BoolNode::Poly(Polynomial {
            cmp: flip_cmp(polynomial.cmp),
            ..polynomial
        })),
        BoolNode::Remainder {
            field,
            divisor,
            remainder,
            cmp,
        } => Ok(BoolNode::Remainder {
            field,
            divisor,
            remainder,
            cmp: flip_cmp(cmp),
        }),
        BoolNode::All(children) => {
            let negated = children
                .into_iter()
                .map(negate_node)
                .collect::<Result<Vec<_>, TranslationError>>()?;
            Ok(BoolNode::Any(negated))
        }
        BoolNode::Any(children) => {
            let negated = children
                .into_iter()
                .map(negate_node)
                .collect::<Result<Vec<_>, TranslationError>>()?;
            Ok(BoolNode::All(negated))
        }
    }
}

fn flip_cmp(cmp: Cmp) -> Cmp {
    match cmp {
        Cmp::Eq => Cmp::Ne,
        Cmp::Ne => Cmp::Eq,
        Cmp::Lt => Cmp::Ge,
        Cmp::Le => Cmp::Gt,
        Cmp::Gt => Cmp::Le,
        Cmp::Ge => Cmp::Lt,
    }
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
    linear_comparison(cmp, lhs, rhs, bounds)
}

/// `lhs (cmp) rhs` as one merged linear atom.
fn linear_comparison(
    cmp: Cmp,
    lhs: &Expr,
    rhs: &Expr,
    bounds: &BTreeMap<String, (i64, i64)>,
) -> Result<Linear, TranslationError> {
    let left = affine_of(lhs, bounds)?;
    let right = affine_of(rhs, bounds)?;
    // l (cmp) r  ⟺  Σ (l.terms - r.terms) (cmp) r.constant - l.constant
    let mut merged = left.terms;
    for (name, coefficient) in right.terms {
        let slot = merged.entry(name).or_insert(0);
        *slot = slot.checked_sub(coefficient).ok_or(IR_INTEGER_OVERFLOW)?;
    }
    let terms: Vec<(String, i64)> = merged
        .into_iter()
        .filter(|(_, coefficient)| *coefficient != 0)
        .collect();
    Ok(Linear {
        terms,
        rhs: right
            .constant
            .checked_sub(left.constant)
            .ok_or(IR_INTEGER_OVERFLOW)?,
        cmp,
    })
}

/// `lhs (cmp) rhs` as one polynomial comparison: both sides expand into
/// monomials and the right side folds into the constant. The product
/// expansion is what the affine translation refuses — two non-constant
/// factors — so this is the non-linear path. The total degree per
/// monomial is capped, and the value magnitude is bounded so the
/// evaluator's `i128` arithmetic cannot overflow (both conservative:
/// anything larger stays `Unsupported`).
fn polynomial_comparison(
    expr: &Expr,
    bounds: &BTreeMap<String, (i64, i64)>,
) -> Result<Polynomial, TranslationError> {
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
    let left = poly_of(lhs, bounds)?;
    let right = poly_of(rhs, bounds)?;
    // l (cmp) r  ⟺  Σ (l.terms - r.terms) (cmp) r.constant - l.constant
    let mut merged = left.terms;
    for (monomial, coefficient) in right.terms {
        let slot = merged.entry(monomial).or_insert(0);
        *slot = slot.checked_sub(coefficient).ok_or(IR_INTEGER_OVERFLOW)?;
    }
    let terms: Vec<(BTreeMap<String, u32>, i64)> = merged
        .into_iter()
        .filter(|(_, coefficient)| *coefficient != 0)
        .collect();
    let polynomial = Polynomial {
        terms,
        rhs: right
            .constant
            .checked_sub(left.constant)
            .ok_or(IR_INTEGER_OVERFLOW)?,
        cmp,
    };
    check_magnitude(&polynomial, bounds)?;
    Ok(polynomial)
}

/// The per-monomial degree cap: `vmax^4` over the solver's `i32`
/// variable domain stays inside `i128`; a fifth power need not.
const MAX_POLY_DEGREE: u32 = 4;

/// The value-magnitude ceiling for one polynomial's worth of monomials.
const POLY_MAGNITUDE_LIMIT: i128 = 1 << 120;

/// The variable-magnitude bound of one monomial:
/// `|coefficient| · Π vmax^exponent`.
fn monomial_magnitude(
    monomial: &BTreeMap<String, u32>,
    coefficient: i64,
    bounds: &BTreeMap<String, (i64, i64)>,
) -> Option<i128> {
    let mut magnitude = i128::from(coefficient).abs();
    for (name, exponent) in monomial {
        let (lower, upper) = bounds.get(name)?;
        let vmax = (*lower).abs().max((*upper).abs()) as i128;
        magnitude = magnitude.checked_mul(vmax.checked_pow(*exponent)?)?;
    }
    Some(magnitude)
}

/// Reject polynomials whose evaluation could overflow the evaluator's
/// `i128` arithmetic: each monomial's bound must exist and the sum of
/// the bounds must stay under the ceiling.
fn check_magnitude(
    polynomial: &Polynomial,
    bounds: &BTreeMap<String, (i64, i64)>,
) -> Result<(), TranslationError> {
    let mut total: i128 = i128::from(polynomial.rhs).abs();
    for (monomial, coefficient) in &polynomial.terms {
        if monomial.values().sum::<u32>() > MAX_POLY_DEGREE {
            return Err(TranslationError::Unsupported(
                "polynomial degree above the supported cap",
            ));
        }
        let Some(magnitude) = monomial_magnitude(monomial, *coefficient, bounds) else {
            return Err(TranslationError::Unsupported(
                "polynomial coefficient magnitude out of range",
            ));
        };
        total = total
            .checked_add(magnitude)
            .ok_or(TranslationError::Unsupported(
                "polynomial value magnitude out of range",
            ))?;
        if total > POLY_MAGNITUDE_LIMIT {
            return Err(TranslationError::Unsupported(
                "polynomial value magnitude out of range",
            ));
        }
    }
    Ok(())
}

/// A non-affine polynomial expression over the problem variables:
/// monomials plus a constant.
#[derive(Debug, Clone)]
struct PolyExpr {
    terms: BTreeMap<BTreeMap<String, u32>, i64>,
    constant: i64,
}

impl PolyExpr {
    fn constant(value: i64) -> Self {
        Self {
            terms: BTreeMap::new(),
            constant: value,
        }
    }

    fn variable(name: &str) -> Self {
        let mut monomial = BTreeMap::new();
        monomial.insert(name.to_owned(), 1);
        let mut terms = BTreeMap::new();
        terms.insert(monomial, 1);
        Self { terms, constant: 0 }
    }

    fn add(&mut self, other: &PolyExpr) -> Result<(), TranslationError> {
        for (monomial, coefficient) in &other.terms {
            let slot = self.terms.entry(monomial.clone()).or_insert(0);
            *slot = slot
                .checked_add(*coefficient)
                .ok_or(TranslationError::Unsupported(
                    "polynomial coefficient overflow",
                ))?;
        }
        self.constant =
            self.constant
                .checked_add(other.constant)
                .ok_or(TranslationError::Unsupported(
                    "polynomial constant overflow",
                ))?;
        Ok(())
    }

    fn subtract(&mut self, other: &PolyExpr) -> Result<(), TranslationError> {
        for (monomial, coefficient) in &other.terms {
            let slot = self.terms.entry(monomial.clone()).or_insert(0);
            *slot = slot
                .checked_sub(*coefficient)
                .ok_or(TranslationError::Unsupported(
                    "polynomial coefficient overflow",
                ))?;
        }
        self.constant =
            self.constant
                .checked_sub(other.constant)
                .ok_or(TranslationError::Unsupported(
                    "polynomial constant overflow",
                ))?;
        Ok(())
    }

    /// The polynomial product: every monomial pair multiplies (exponent
    /// maps merge, coefficients multiply), constants scale each side.
    fn multiply(&self, other: &PolyExpr) -> Result<PolyExpr, TranslationError> {
        let mut result = PolyExpr::constant(0);
        for (left_monomial, left_coefficient) in &self.terms {
            for (right_monomial, right_coefficient) in &other.terms {
                let mut monomial = left_monomial.clone();
                for (name, exponent) in right_monomial {
                    *monomial.entry(name.clone()).or_insert(0) += exponent;
                    if monomial.values().sum::<u32>() > MAX_POLY_DEGREE {
                        return Err(TranslationError::Unsupported(
                            "polynomial degree above the supported cap",
                        ));
                    }
                }
                let coefficient = left_coefficient.checked_mul(*right_coefficient).ok_or(
                    TranslationError::Unsupported("polynomial coefficient overflow"),
                )?;
                let slot = result.terms.entry(monomial).or_insert(0);
                *slot = slot.checked_add(coefficient).ok_or(IR_INTEGER_OVERFLOW)?;
            }
            // left-term × right-constant
            let scaled = left_coefficient.checked_mul(other.constant).ok_or(
                TranslationError::Unsupported("polynomial coefficient overflow"),
            )?;
            if scaled != 0 {
                let monomial = left_monomial.clone();
                let slot = result.terms.entry(monomial).or_insert(0);
                *slot = slot.checked_add(scaled).ok_or(IR_INTEGER_OVERFLOW)?;
            }
        }
        // left-constant × right-terms
        if self.constant != 0 {
            for (right_monomial, right_coefficient) in &other.terms {
                let scaled = self.constant.checked_mul(*right_coefficient).ok_or(
                    TranslationError::Unsupported("polynomial coefficient overflow"),
                )?;
                let monomial = right_monomial.clone();
                let slot = result.terms.entry(monomial).or_insert(0);
                *slot = slot.checked_add(scaled).ok_or(IR_INTEGER_OVERFLOW)?;
            }
        }
        result.constant =
            self.constant
                .checked_mul(other.constant)
                .ok_or(TranslationError::Unsupported(
                    "polynomial constant overflow",
                ))?;
        result.terms.retain(|_, coefficient| *coefficient != 0);
        Ok(result)
    }
}

/// The polynomial expansion of one comparison side.
fn poly_of(
    expr: &Expr,
    bounds: &BTreeMap<String, (i64, i64)>,
) -> Result<PolyExpr, TranslationError> {
    match expr {
        Expr::IntLiteral { text, .. } => {
            let value = crate::interp::parse_int_literal(text)
                .and_then(|v| i64::try_from(v).ok())
                .ok_or(TranslationError::Unsupported(
                    "integer literal out of range",
                ))?;
            Ok(PolyExpr::constant(value))
        }
        Expr::BitLiteral { text, .. } => {
            let value = crate::interp::parse_int_literal(text)
                .and_then(|v| i64::try_from(v).ok())
                .ok_or(TranslationError::Unsupported("based literal out of range"))?;
            Ok(PolyExpr::constant(value))
        }
        Expr::Ident { name, .. } => {
            if bounds.contains_key(name) {
                Ok(PolyExpr::variable(name))
            } else {
                Err(TranslationError::Unsupported(
                    "constraint references a non-integer or unknown identifier",
                ))
            }
        }
        Expr::Paren { expr, .. } => poly_of(expr, bounds),
        Expr::Unary {
            op: crate::ast::UnaryOp::Negate,
            expr,
            ..
        } => {
            let mut result = PolyExpr::constant(0);
            result.subtract(&poly_of(expr, bounds)?)?;
            Ok(result)
        }
        Expr::Unary {
            op: crate::ast::UnaryOp::Plus,
            expr,
            ..
        } => poly_of(expr, bounds),
        Expr::Binary {
            op: crate::ast::BinaryOp::Add,
            lhs,
            rhs,
            ..
        } => {
            let mut left = poly_of(lhs, bounds)?;
            left.add(&poly_of(rhs, bounds)?)?;
            Ok(left)
        }
        Expr::Binary {
            op: crate::ast::BinaryOp::Sub,
            lhs,
            rhs,
            ..
        } => {
            let mut left = poly_of(lhs, bounds)?;
            left.subtract(&poly_of(rhs, bounds)?)?;
            Ok(left)
        }
        Expr::Binary {
            op: crate::ast::BinaryOp::Mul,
            lhs,
            rhs,
            ..
        } => poly_of(lhs, bounds)?.multiply(&poly_of(rhs, bounds)?),
        _ => Err(TranslationError::Unsupported(
            "constraint contains a non-arithmetic expression",
        )),
    }
}

const IR_INTEGER_OVERFLOW: TranslationError =
    TranslationError::Unsupported("solver coefficient or constant out of range");

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

    fn add(mut self, other: &Affine) -> Result<Self, TranslationError> {
        for (name, coefficient) in &other.terms {
            let slot = self.terms.entry(name.clone()).or_insert(0);
            *slot = slot.checked_add(*coefficient).ok_or(IR_INTEGER_OVERFLOW)?;
        }
        self.constant = self
            .constant
            .checked_add(other.constant)
            .ok_or(IR_INTEGER_OVERFLOW)?;
        Ok(self)
    }

    fn subtract(mut self, other: &Affine) -> Result<Self, TranslationError> {
        for (name, coefficient) in &other.terms {
            let slot = self.terms.entry(name.clone()).or_insert(0);
            *slot = slot.checked_sub(*coefficient).ok_or(IR_INTEGER_OVERFLOW)?;
        }
        self.constant = self
            .constant
            .checked_sub(other.constant)
            .ok_or(IR_INTEGER_OVERFLOW)?;
        Ok(self)
    }

    fn scale(mut self, factor: i64) -> Result<Self, TranslationError> {
        for coefficient in self.terms.values_mut() {
            *coefficient = coefficient.checked_mul(factor).ok_or(IR_INTEGER_OVERFLOW)?;
        }
        self.constant = self
            .constant
            .checked_mul(factor)
            .ok_or(IR_INTEGER_OVERFLOW)?;
        Ok(self)
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
        } => affine_of(expr, bounds)?.scale(-1),
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
        } => affine_of(lhs, bounds)?.add(&affine_of(rhs, bounds)?),
        Expr::Binary {
            op: crate::ast::BinaryOp::Sub,
            lhs,
            rhs,
            ..
        } => affine_of(lhs, bounds)?.subtract(&affine_of(rhs, bounds)?),
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
                right.scale(left.constant)
            } else if right.terms.is_empty() {
                left.scale(right.constant)
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

/// Branch combinations expand beyond this and [`solve`] reports
/// [`SolveOutcome::Unsupported`] — the rejection sampler enforces such a
/// constraint list by evaluation instead, and the exponential blowup is
/// not worth solving.
const MAX_BRANCH_COMBINATIONS: usize = 64;

/// The bounded enumeration solver's domain-product cap: a problem over
/// more candidate tuples defers to the rejection sampler instead of
/// exploding.
const MAX_ENUMERATION_STATES: u128 = 1 << 20;

/// Solve a problem using the default [`Solver::Auto`] policy. The
/// polynomial path uses bounded enumeration. The linear path expands
/// disjunctions up to the branch limit and passes each conjunction to
/// Pumpkin when available. Expansion can prove an empty disjunction
/// unsatisfiable in either build.
pub fn solve(problem: &Problem, seed: u64) -> SolveOutcome {
    Solver::Auto.solve(problem, seed)
}

fn solve_with_pumpkin_branches(problem: &Problem, seed: u64) -> SolveOutcome {
    let Some(combos) = conjunction_alternatives(&problem.constraints) else {
        return SolveOutcome::Unsupported;
    };
    let mut satisfying: Vec<Vec<i128>> = Vec::new();
    let mut any_unsupported = false;
    for combo in &combos {
        let flattened = Problem {
            vars: problem.vars.clone(),
            constraints: combo.clone(),
        };
        match solve_conjunction(&flattened, seed) {
            SolveOutcome::Solved(values) => satisfying.push(values),
            SolveOutcome::Unsat => {}
            SolveOutcome::Unsupported => any_unsupported = true,
        }
    }
    if !satisfying.is_empty() {
        let values = &satisfying[(seed % satisfying.len() as u64) as usize];
        return SolveOutcome::Solved(values.clone());
    }
    if any_unsupported {
        SolveOutcome::Unsupported
    } else {
        SolveOutcome::Unsat
    }
}

/// The bounded enumeration solver: the declared domains' product is
/// capped, the constraint tree is enumerated depth-first (variables in
/// name order, values ascending — so the k-th solution is a stable
/// choice), subtrees whose fully-assigned constraints already fail are
/// pruned, and the `seed`-th satisfying assignment is the answer. Two
/// passes over the capped space — count, then walk to the target index —
/// keep the memory flat.
fn solve_by_enumeration(problem: &Problem, seed: u64) -> SolveOutcome {
    let mut domain_product: u128 = 1;
    for var in &problem.vars {
        if var.lower > var.upper {
            return SolveOutcome::Unsat;
        }
        let size = (i128::from(var.upper) - i128::from(var.lower) + 1) as u128;
        domain_product = domain_product.saturating_mul(size);
        if domain_product > MAX_ENUMERATION_STATES {
            return SolveOutcome::Unsupported;
        }
    }
    // Pass one: count the satisfying assignments.
    let mut count: u128 = 0;
    if enumerate_solutions(problem, &mut |_, _| {
        count = count.saturating_add(1);
        true
    })
    .is_err()
    {
        return SolveOutcome::Unsupported;
    }
    if count == 0 {
        return SolveOutcome::Unsat;
    }
    // Pass two: walk to the seed-th solution in enumeration order.
    let target = u128::from(seed) % count;
    let mut picked: Option<Vec<i128>> = None;
    let walked = enumerate_solutions(problem, &mut |index, assignment| {
        if index as u128 == target {
            picked = Some(assignment.to_vec());
            false
        } else {
            true
        }
    });
    match (walked, picked) {
        (Ok(()), Some(assignment)) => SolveOutcome::Solved(assignment),
        // The count pass found the target; the walk must too. A miss is
        // an internal inconsistency — defer to the rejection sampler
        // rather than report a wrong verdict.
        _ => SolveOutcome::Unsupported,
    }
}

/// Depth-first enumeration over the declared domains in variable order.
/// `visit` receives each satisfying assignment with its 0-based index in
/// enumeration order and returns whether to continue; `true` continues,
/// `false` stops the walk.
fn enumerate_solutions(
    problem: &Problem,
    visit: &mut dyn FnMut(usize, &[i128]) -> bool,
) -> Result<(), ()> {
    let vars = &problem.vars;
    let mut assignment: Vec<i64> = vec![0; vars.len()];
    let mut count: usize = 0;
    walk(
        0,
        vars,
        &problem.constraints,
        &mut assignment,
        &mut count,
        visit,
    )
    .map(|_| ())
}

/// Return true when the visitor requests an early stop.
fn walk(
    depth: usize,
    vars: &[BoundedInt],
    constraints: &[BoolNode],
    assignment: &mut [i64],
    count: &mut usize,
    visit: &mut dyn FnMut(usize, &[i128]) -> bool,
) -> Result<bool, ()> {
    let Some(var) = vars.get(depth) else {
        // Every variable assigned: the tree evaluates to a plain
        // verdict (the partial evaluator resolves every atom now).
        let map: BTreeMap<&str, i128> = vars
            .iter()
            .zip(assignment.iter())
            .map(|(var, value)| (var.name.as_str(), i128::from(*value)))
            .collect();
        return match eval_partial(constraints, &map) {
            PartialEval::True => {
                let index = *count;
                *count += 1;
                let snapshot: Vec<i128> =
                    assignment.iter().map(|value| i128::from(*value)).collect();
                Ok(!visit(index, &snapshot))
            }
            PartialEval::False => Ok(false),
            PartialEval::Unknown => Err(()),
        };
    };
    for value in var.lower..=var.upper {
        assignment[depth] = value;
        let map: BTreeMap<&str, i128> = vars
            .iter()
            .take(depth + 1)
            .zip(assignment.iter())
            .map(|(var, value)| (var.name.as_str(), i128::from(*value)))
            .collect();
        // Partial pruning: a subtree whose fully-assigned constraints
        // already fail cannot produce a solution.
        if matches!(eval_partial(constraints, &map), PartialEval::False) {
            continue;
        }
        if walk(depth + 1, vars, constraints, assignment, count, visit)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The three-valued verdict of a constraint tree over a partial
/// assignment: an atom whose variables are all assigned decides, a tree
/// decides when its children do (conjunction/disjunction semantics),
/// anything else stays unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PartialEval {
    True,
    False,
    Unknown,
}

fn eval_partial(nodes: &[BoolNode], map: &BTreeMap<&str, i128>) -> PartialEval {
    // An empty constraint list is trivially true (a conjunction over
    // nothing).
    let mut all_true = true;
    for node in nodes {
        match eval_partial_node(node, map) {
            PartialEval::True => {}
            PartialEval::False => return PartialEval::False,
            PartialEval::Unknown => all_true = false,
        }
    }
    if all_true {
        PartialEval::True
    } else {
        PartialEval::Unknown
    }
}

fn eval_partial_node(node: &BoolNode, map: &BTreeMap<&str, i128>) -> PartialEval {
    match node {
        BoolNode::All(children) => {
            let mut all_true = true;
            for child in children {
                match eval_partial_node(child, map) {
                    PartialEval::True => {}
                    PartialEval::False => return PartialEval::False,
                    PartialEval::Unknown => all_true = false,
                }
            }
            if all_true {
                PartialEval::True
            } else {
                PartialEval::Unknown
            }
        }
        BoolNode::Any(children) => {
            let mut all_false = true;
            for child in children {
                match eval_partial_node(child, map) {
                    PartialEval::True => return PartialEval::True,
                    PartialEval::False => {}
                    PartialEval::Unknown => all_false = false,
                }
            }
            if all_false {
                PartialEval::False
            } else {
                PartialEval::Unknown
            }
        }
        BoolNode::Atom(linear) => match linear_value(linear, map) {
            Some(value) => holds(linear.cmp, value),
            None => PartialEval::Unknown,
        },
        BoolNode::Poly(polynomial) => match polynomial_value(polynomial, map) {
            Some(value) => holds(polynomial.cmp, value),
            None => PartialEval::Unknown,
        },
        BoolNode::Remainder {
            field,
            divisor,
            remainder,
            cmp,
        } => match map
            .get(field.as_str())
            .and_then(|value| value.checked_rem(i128::from(*divisor)))
        {
            Some(value) => holds(*cmp, value - i128::from(*remainder)),
            None => PartialEval::Unknown,
        },
    }
}

fn holds(cmp: Cmp, value: i128) -> PartialEval {
    match cmp {
        Cmp::Eq => bool_eval(value == 0),
        Cmp::Ne => bool_eval(value != 0),
        Cmp::Lt => bool_eval(value < 0),
        Cmp::Le => bool_eval(value <= 0),
        Cmp::Gt => bool_eval(value > 0),
        Cmp::Ge => bool_eval(value >= 0),
    }
}

fn bool_eval(value: bool) -> PartialEval {
    if value {
        PartialEval::True
    } else {
        PartialEval::False
    }
}

/// `Σ coefficient·variable - rhs` over the assigned variables; `None`
/// when a variable is unassigned or the arithmetic overflows (the
/// translation-time magnitude check makes the overflow unreachable in
/// practice).
fn linear_value(linear: &Linear, map: &BTreeMap<&str, i128>) -> Option<i128> {
    let mut value = -i128::from(linear.rhs);
    for (name, coefficient) in &linear.terms {
        let assigned = map.get(name.as_str())?;
        value = value.checked_add(i128::from(*coefficient).checked_mul(*assigned)?)?;
    }
    Some(value)
}

/// `Σ coefficient·Π variable^exponent - rhs`; `None` on an unassigned
/// variable or an overflow.
fn polynomial_value(polynomial: &Polynomial, map: &BTreeMap<&str, i128>) -> Option<i128> {
    let mut value = -i128::from(polynomial.rhs);
    for (monomial, coefficient) in &polynomial.terms {
        let mut product = i128::from(*coefficient);
        for (name, exponent) in monomial {
            let assigned = map.get(name.as_str())?;
            product = product.checked_mul(assigned.checked_pow(*exponent)?)?;
        }
        value = value.checked_add(product)?;
    }
    Some(value)
}

/// Expand a linear node into flat conjunctions within the branch budget.
fn branch_alternatives(node: &BoolNode) -> Option<Vec<Vec<BoolNode>>> {
    match node {
        BoolNode::Atom(_) | BoolNode::Remainder { .. } => Some(vec![vec![node.clone()]]),
        BoolNode::Poly(_) => None,
        BoolNode::All(children) => conjunction_alternatives(children),
        BoolNode::Any(branches) => {
            let mut alternatives = Vec::new();
            for branch in branches {
                let expanded = branch_alternatives(branch)?;
                if alternatives.len() + expanded.len() > MAX_BRANCH_COMBINATIONS {
                    return None;
                }
                alternatives.extend(expanded);
            }
            Some(alternatives)
        }
    }
}

fn conjunction_alternatives(nodes: &[BoolNode]) -> Option<Vec<Vec<BoolNode>>> {
    let mut combinations = vec![Vec::new()];
    for node in nodes {
        let alternatives = branch_alternatives(node)?;
        let count = combinations.len() * alternatives.len();
        if count > MAX_BRANCH_COMBINATIONS {
            return None;
        }
        if let [only] = alternatives.as_slice() {
            for combination in &mut combinations {
                combination.extend_from_slice(only);
            }
            continue;
        }
        let mut next = Vec::with_capacity(count);
        for combination in &combinations {
            for alternative in &alternatives {
                let mut expanded = combination.clone();
                expanded.extend_from_slice(alternative);
                next.push(expanded);
            }
        }
        combinations = next;
    }
    Some(combinations)
}

/// Solve one flattened conjunction with the best available backend.
fn solve_conjunction(problem: &Problem, seed: u64) -> SolveOutcome {
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
    use super::{BoolNode, Cmp, Problem, SolveOutcome, remainder_bounds};
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
            if var.lower > var.upper {
                return SolveOutcome::Unsat;
            }
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
            BoolNode::Remainder {
                field,
                divisor,
                remainder,
                cmp,
            } => {
                let &(lower, upper) = bounds.get(field.as_str()).ok_or(())?;
                if *divisor <= 0 || *divisor > I32_LIMIT || i32::try_from(*remainder).is_err() {
                    return Err(());
                }
                let variable = variables[field.as_str()];
                if lower < 0 || *cmp != Cmp::Eq {
                    let (quotient_low, quotient_high) = (lower / divisor, upper / divisor);
                    let (residual_low, residual_high) = remainder_bounds(
                        *cmp,
                        *remainder,
                        lower.min(0).max(1 - divisor),
                        upper.max(0).min(divisor - 1),
                    );
                    if residual_low > residual_high
                        || lower - quotient_high * divisor - residual_high < i64::from(i32::MIN)
                        || upper - quotient_low * divisor - residual_low > i64::from(i32::MAX)
                    {
                        return Err(());
                    }
                    let quotient =
                        solver.new_bounded_integer(quotient_low as i32, quotient_high as i32);
                    if lower < 0 {
                        // The linear equation alone would allow the wrong
                        // residual sign. Enforce truncation toward zero.
                        let denominator =
                            solver.new_bounded_integer(*divisor as i32, *divisor as i32);
                        let tag = solver.new_constraint_tag();
                        solver
                            .add_constraint(pumpkin_constraints::division(
                                variable,
                                denominator,
                                quotient,
                                tag,
                            ))
                            .post();
                    }
                    let residual =
                        solver.new_bounded_integer(residual_low as i32, residual_high as i32);
                    let tag = solver.new_constraint_tag();
                    solver
                        .add_constraint(pumpkin_constraints::equals(
                            [
                                variable.scaled(1),
                                quotient.scaled(-(*divisor as i32)),
                                residual.scaled(-1),
                            ],
                            0,
                            tag,
                        ))
                        .post();
                    if *cmp == Cmp::Ne && (residual_low..=residual_high).contains(remainder) {
                        let tag = solver.new_constraint_tag();
                        solver
                            .add_constraint(pumpkin_constraints::not_equals(
                                [residual],
                                *remainder as i32,
                                tag,
                            ))
                            .post();
                    }
                    return Ok(());
                }
                // x = divisor * quotient + remainder. Bound the auxiliary
                // quotient using the first and last possible matching values.
                if !(0..*divisor).contains(remainder) {
                    return Err(());
                }
                let first = lower + (*remainder - lower).rem_euclid(*divisor);
                if first > upper {
                    return Err(());
                }
                let quotient = solver.new_bounded_integer(
                    ((first - remainder) / divisor) as i32,
                    ((upper - remainder) / divisor) as i32,
                );
                let tag = solver.new_constraint_tag();
                solver
                    .add_constraint(pumpkin_constraints::equals(
                        [variable.scaled(1), quotient.scaled(-(*divisor as i32))],
                        *remainder as i32,
                        tag,
                    ))
                    .post();
                Ok(())
            }
            // `solve` expands every `Any` into branch combinations and
            // routes every polynomial problem to the enumeration solver
            // before the backend sees a problem; reaching one here is a
            // bug, and the caller falls back to rejection sampling for
            // it.
            BoolNode::Any(_) | BoolNode::Poly(_) => Err(()),
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
        if !(-I32_LIMIT..=I32_LIMIT).contains(&linear.rhs) {
            return Err(());
        }
        // The sum of scaled terms must stay inside the solver's i32 domain.
        let mut lower_sum: i64 = 0;
        let mut upper_sum: i64 = 0;
        for (name, coefficient) in &linear.terms {
            if !(-I32_LIMIT..=I32_LIMIT).contains(coefficient) {
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
