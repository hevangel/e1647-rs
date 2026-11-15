//! Conservative, total-result interval proofs for coverage predicates.
//! Unknown results retain ordinary evaluation and bounded domain splitting.

use super::{bare_item, uses_item};
use crate::{
    ast::{BinaryOp, Expr, InSetExpr, UnaryOp},
    diagnostic::Diagnostic,
    interp::Value,
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
pub(crate) enum ScalarDomain {
    Int(i128, i128),
    Bool(Option<bool>),
}

impl ScalarDomain {
    pub(crate) fn new(prototype: &Value, (lo, hi): (i128, i128)) -> Self {
        if matches!(prototype, Value::Bool(_)) {
            Self::Bool(if lo == 0 && hi == 0 {
                Some(false)
            } else if hi < 0 || lo > 0 {
                Some(true)
            } else {
                None
            })
        } else {
            Self::Int(lo, hi)
        }
    }

    fn fixed(self) -> bool {
        match self {
            Self::Int(lo, hi) => lo == hi,
            Self::Bool(value) => value.is_some(),
        }
    }

    fn join(self, other: Self) -> Option<Self> {
        match (self, other) {
            (Self::Int(a, b), Self::Int(c, d)) => Some(Self::Int(a.min(c), b.max(d))),
            (Self::Bool(a), Self::Bool(b)) => Some(Self::Bool(if a == b { a } else { None })),
            _ => None,
        }
    }
}

pub(crate) type ConstantEval<'a> = dyn FnMut(&Expr) -> Result<Value, Diagnostic> + 'a;

/// Some proves the same Boolean result, without errors, throughout the box.
/// None includes unsupported shapes and possible operand/evaluation errors.
pub(crate) fn prove_predicate(
    expr: &Expr,
    domains: &BTreeMap<String, ScalarDomain>,
    evaluate: &mut ConstantEval<'_>,
) -> Result<Option<bool>, Diagnostic> {
    Ok(match domain(expr, domains, evaluate)? {
        Some(ScalarDomain::Bool(value)) => value,
        _ => None,
    })
}

fn domain(
    expr: &Expr,
    domains: &BTreeMap<String, ScalarDomain>,
    evaluate: &mut ConstantEval<'_>,
) -> Result<Option<ScalarDomain>, Diagnostic> {
    use ScalarDomain::{Bool, Int};
    // This also resolves arbitrary supported pure expressions on singleton
    // inputs using the actual interpreter, preserving its operand semantics.
    if domains
        .iter()
        .all(|(name, bounds)| bounds.fixed() || !uses_item(expr, name))
    {
        return Ok(match evaluate(expr)? {
            Value::Bool(value) => Some(Bool(Some(value))),
            Value::Int(value) => Some(Int(value, value)),
            _ => None,
        });
    }
    Ok(match expr {
        Expr::Ident { name, .. } => domains.get(name).copied(),
        Expr::Paren { expr, .. } => domain(expr, domains, evaluate)?,
        Expr::Unary { op, expr, .. } => match (op, domain(expr, domains, evaluate)?) {
            (UnaryOp::Not, Some(Bool(value))) => Some(Bool(value.map(|value| !value))),
            (UnaryOp::Plus, Some(value @ Int(..))) => Some(value),
            (UnaryOp::Negate, Some(Int(lo, hi))) => hi
                .checked_neg()
                .zip(lo.checked_neg())
                .map(|(lo, hi)| Int(lo, hi)),
            (UnaryOp::BitNot, Some(Int(lo, hi))) => Some(Int(!hi, !lo)),
            _ => None,
        },
        Expr::Binary { op, lhs, rhs, .. }
            if matches!(
                op,
                BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::Imply
            ) =>
        {
            match domain(lhs, domains, evaluate)? {
                Some(Bool(Some(left))) => match op {
                    BinaryOp::LogicalAnd if !left => Some(Bool(Some(false))),
                    BinaryOp::LogicalOr if left => Some(Bool(Some(true))),
                    BinaryOp::Imply if !left => Some(Bool(Some(true))),
                    _ => match domain(rhs, domains, evaluate)? {
                        Some(value @ Bool(_)) => Some(value),
                        _ => None,
                    },
                },
                Some(Bool(None)) => {
                    // An error on the right might be shielded by a short
                    // circuit. It cannot become an unconditional error here.
                    match domain(rhs, domains, evaluate).ok().flatten() {
                        Some(Bool(right)) => Some(Bool(match op {
                            BinaryOp::LogicalAnd if right == Some(false) => Some(false),
                            BinaryOp::LogicalOr | BinaryOp::Imply if right == Some(true) => {
                                Some(true)
                            }
                            _ => None,
                        })),
                        _ => None,
                    }
                }
                _ => None,
            }
        }
        Expr::Binary { op, lhs, rhs, .. } => {
            let Some(left) = domain(lhs, domains, evaluate)? else {
                return Ok(None);
            };
            let Some(right) = domain(rhs, domains, evaluate)? else {
                return Ok(None);
            };
            let same = domains
                .keys()
                .any(|name| bare_item(lhs, name) && bare_item(rhs, name));
            match op {
                BinaryOp::Eq
                | BinaryOp::NotEq
                | BinaryOp::Lt
                | BinaryOp::Le
                | BinaryOp::Gt
                | BinaryOp::Ge => compare(*op, left, right, same).map(Bool),
                _ => arithmetic(*op, left, right),
            }
        }
        Expr::Ternary {
            cond,
            then_expr,
            else_expr,
            ..
        } => match domain(cond, domains, evaluate)? {
            Some(Bool(Some(true))) => domain(then_expr, domains, evaluate)?,
            Some(Bool(Some(false))) => domain(else_expr, domains, evaluate)?,
            Some(Bool(None)) => {
                let left = domain(then_expr, domains, evaluate).ok().flatten();
                let right = domain(else_expr, domains, evaluate).ok().flatten();
                left.zip(right).and_then(|(left, right)| left.join(right))
            }
            _ => None,
        },
        Expr::InSet { element, set, .. } => {
            let Some(value) = domain(element, domains, evaluate)? else {
                return Ok(None);
            };
            match set {
                InSetExpr::Range(lo, hi) => {
                    let Some(Int(a, b)) = domain(lo, domains, evaluate)? else {
                        return Ok(None);
                    };
                    let Some(Int(c, d)) = domain(hi, domains, evaluate)? else {
                        return Ok(None);
                    };
                    match value {
                        Int(lo, hi) if lo >= b && hi <= c => Some(Bool(Some(true))),
                        Int(lo, hi) if hi < a || lo > d || a > d => Some(Bool(Some(false))),
                        Int(..) => Some(Bool(None)),
                        _ => None,
                    }
                }
                InSetExpr::Items(items) => {
                    let mut uncertain = false;
                    for item in items {
                        let result = domain(item, domains, evaluate);
                        let other = if uncertain {
                            result.ok().flatten()
                        } else {
                            result?
                        };
                        let Some(other) = other else {
                            return Ok(None);
                        };
                        let same = domains
                            .keys()
                            .any(|name| bare_item(element, name) && bare_item(item, name));
                        match compare(BinaryOp::Eq, value, other, same) {
                            Some(Some(false)) => {}
                            Some(Some(true)) => {
                                return Ok(Some(Bool(Some(true))));
                            }
                            Some(None) => uncertain = true,
                            None => return Ok(None),
                        }
                    }
                    Some(Bool(if uncertain { None } else { Some(false) }))
                }
                _ => None,
            }
        }
        _ => None,
    })
}

/// Outer None means the comparison may have invalid operand kinds. Inner
/// None is an unknown but total Boolean result.
fn compare(
    op: BinaryOp,
    left: ScalarDomain,
    right: ScalarDomain,
    same: bool,
) -> Option<Option<bool>> {
    use BinaryOp::*;
    use ScalarDomain::{Bool, Int};
    let value = match (left, right) {
        (Int(a, b), Int(c, d)) => {
            if same {
                return Some(Some(matches!(op, Eq | Le | Ge)));
            }
            match op {
                Eq if b < c || d < a => Some(false),
                Eq if a == b && c == d && a == c => Some(true),
                NotEq => {
                    return compare(Eq, left, right, false).map(|value| value.map(|value| !value));
                }
                Lt if b < c => Some(true),
                Lt if a >= d => Some(false),
                Le if b <= c => Some(true),
                Le if a > d => Some(false),
                Gt => return compare(Lt, right, left, false),
                Ge => return compare(Le, right, left, false),
                _ => None,
            }
        }
        (Bool(a), Bool(b)) if matches!(op, Eq | NotEq) => {
            if same {
                Some(op == Eq)
            } else {
                a.zip(b)
                    .map(|(a, b)| if op == Eq { a == b } else { a != b })
            }
        }
        _ => return None,
    };
    Some(value)
}

fn arithmetic(op: BinaryOp, left: ScalarDomain, right: ScalarDomain) -> Option<ScalarDomain> {
    use ScalarDomain::Int;
    let (Int(a, b), Int(c, d)) = (left, right) else {
        return None;
    };
    let (lo, hi) = match op {
        BinaryOp::Add => (a.checked_add(c)?, b.checked_add(d)?),
        BinaryOp::Sub => (a.checked_sub(d)?, b.checked_sub(c)?),
        BinaryOp::Mul | BinaryOp::Div => {
            if op == BinaryOp::Div && c <= 0 && d >= 0 {
                return None;
            }
            let operation = |a: i128, b: i128| {
                if op == BinaryOp::Mul {
                    a.checked_mul(b)
                } else {
                    a.checked_div(b)
                }
            };
            let values = [
                operation(a, c)?,
                operation(a, d)?,
                operation(b, c)?,
                operation(b, d)?,
            ];
            (*values.iter().min()?, *values.iter().max()?)
        }
        _ => return None,
    };
    Some(Int(lo, hi))
}
