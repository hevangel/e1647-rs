use e1647::ast::Expr;
use e1647::solve::{SolveOutcome, TranslationError, int_bounds, solve, translate};
use std::collections::BTreeMap;

fn bounds_from(pairs: &[(&str, (i64, i64))]) -> BTreeMap<String, (i64, i64)> {
    pairs
        .iter()
        .map(|(name, bounds)| (name.to_string(), *bounds))
        .collect()
}

fn parse_exprs(source: &str) -> Vec<Expr> {
    // Wrap the expressions in keep constraints of one struct and pull them
    // out of the AST.
    let wrapped = format!("struct s {{}};\nextend s {{ {} }};\n", {
        // One keep per line of input.
        source
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| format!("keep {};", line))
            .collect::<Vec<_>>()
            .join("\n")
    });
    let parsed = e1647::parser::parse(&wrapped);
    assert!(
        parsed.diagnostics.is_empty(),
        "parse diagnostics: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
    let extend = parsed
        .declarations
        .iter()
        .find(|d| d.name == "s" && d.kind == e1647::ast::DeclarationKind::Extend)
        .expect("extend s present");
    extend
        .members
        .iter()
        .filter_map(|member| match member {
            e1647::ast::Member::Constraint(constraint) => Some(constraint.expr.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn int_bounds_cover_integer_scalars_only() {
    let parsed = e1647::parser::parse(
        "struct t {\n    a: uint(bits: 8);\n    b: int;\n    c: byte;\n    d: nibble;\n    e: bool;\n    f: list of uint;\n};\n",
    );
    assert!(parsed.diagnostics.is_empty());
    let fields: Vec<(String, Option<(i64, i64)>)> = parsed.declarations[0]
        .members
        .iter()
        .filter_map(|member| match member {
            e1647::ast::Member::Field(field) => Some((field.name.clone(), int_bounds(&field.ty))),
            _ => None,
        })
        .collect();
    assert_eq!(fields[0], ("a".to_string(), Some((0, 255))));
    assert_eq!(
        fields[1],
        ("b".to_string(), Some((-(1i64 << 30), (1i64 << 30) - 1)))
    );
    assert_eq!(fields[2], ("c".to_string(), Some((0, 255))));
    assert_eq!(fields[3], ("d".to_string(), Some((0, 15))));
    assert_eq!(fields[4], ("e".to_string(), None));
    assert_eq!(fields[5], ("f".to_string(), None));
}

#[test]
fn translate_linear_equality() {
    let exprs = parse_exprs("a + b == 100");
    let problem =
        translate(&exprs, &bounds_from(&[("a", (0, 100)), ("b", (0, 100))])).expect("translatable");
    assert_eq!(problem.vars.len(), 2);
    assert_eq!(problem.constraints.len(), 1);
    let e1647::solve::BoolNode::Atom(linear) = &problem.constraints[0] else {
        panic!("expected an atom");
    };
    assert_eq!(
        linear.terms,
        vec![("a".to_string(), 1), ("b".to_string(), 1)]
    );
    assert_eq!(linear.rhs, 100);
    assert_eq!(linear.cmp, e1647::solve::Cmp::Eq);
}

#[test]
fn translate_moves_rhs_constants_and_merges_terms() {
    let exprs = parse_exprs("2*a - b + 5 >= b - 3");
    let problem =
        translate(&exprs, &bounds_from(&[("a", (0, 50)), ("b", (0, 50))])).expect("translatable");
    let e1647::solve::BoolNode::Atom(linear) = &problem.constraints[0] else {
        panic!("expected an atom");
    };
    // 2a - b + 5 >= b - 3  ⟺  2a - 2b >= -8
    assert_eq!(
        linear.terms,
        vec![("a".to_string(), 2), ("b".to_string(), -2)]
    );
    assert_eq!(linear.rhs, -8);
    assert_eq!(linear.cmp, e1647::solve::Cmp::Ge);
}

#[test]
fn translate_splits_conjunctions() {
    let exprs = parse_exprs("a < 10\nb > 5");
    let problem =
        translate(&exprs, &bounds_from(&[("a", (0, 20)), ("b", (0, 20))])).expect("translatable");
    assert_eq!(problem.constraints.len(), 2);
}

#[test]
fn translate_range_membership_into_two_comparisons() {
    let exprs = parse_exprs("a in [1..100]");
    let problem = translate(&exprs, &bounds_from(&[("a", (0, 255))])).expect("translatable");
    let e1647::solve::BoolNode::All(atoms) = &problem.constraints[0] else {
        panic!("expected the range to translate as a conjunction");
    };
    assert_eq!(atoms.len(), 2);
    let e1647::solve::BoolNode::Atom(low) = &atoms[0] else {
        panic!("expected the lower bound atom");
    };
    // a >= 1
    assert_eq!(low.terms, vec![("a".to_string(), 1)]);
    assert_eq!(low.rhs, 1);
    assert_eq!(low.cmp, e1647::solve::Cmp::Ge);
    let e1647::solve::BoolNode::Atom(high) = &atoms[1] else {
        panic!("expected the upper bound atom");
    };
    // a <= 100
    assert_eq!(high.terms, vec![("a".to_string(), 1)]);
    assert_eq!(high.rhs, 100);
    assert_eq!(high.cmp, e1647::solve::Cmp::Le);
}

#[test]
fn translate_item_membership_is_unsupported() {
    // Item-list membership needs a disjunction; the IR has none yet and
    // the caller falls back to the evaluation-based rejection sampler.
    let exprs = parse_exprs("a in [1, 5, 9]");
    let error = translate(&exprs, &bounds_from(&[("a", (0, 255))])).expect_err("unsupported");
    assert_eq!(
        error,
        TranslationError::Unsupported("set membership over an item list")
    );
}

#[test]
fn translate_range_with_expression_bounds() {
    // The bounds go through the same affine translation as comparisons.
    let exprs = parse_exprs("a in [2*2..a_max - 1]");
    let problem = translate(&exprs, &bounds_from(&[("a", (0, 50)), ("a_max", (0, 50))]))
        .expect("translatable");
    let e1647::solve::BoolNode::All(atoms) = &problem.constraints[0] else {
        panic!("expected the range to translate as a conjunction");
    };
    let e1647::solve::BoolNode::Atom(low) = &atoms[0] else {
        panic!("expected the lower bound atom");
    };
    // a >= 4
    assert_eq!(low.rhs, 4);
    assert_eq!(low.cmp, e1647::solve::Cmp::Ge);
    let e1647::solve::BoolNode::Atom(high) = &atoms[1] else {
        panic!("expected the upper bound atom");
    };
    // a - a_max <= -1  ⟺  a <= a_max - 1
    assert_eq!(
        high.terms,
        vec![("a".to_string(), 1), ("a_max".to_string(), -1)]
    );
    assert_eq!(high.rhs, -1);
    assert_eq!(high.cmp, e1647::solve::Cmp::Le);
}

#[test]
fn translate_rejects_unsupported_shapes() {
    let bounds = bounds_from(&[("a", (0, 10))]);
    // Disjunction.
    let exprs = parse_exprs("a == 1 || a == 2");
    assert_eq!(
        translate(&exprs, &bounds),
        Err(TranslationError::Unsupported(
            "constraint operator is not a comparison"
        ))
    );
    // Non-linear multiplication.
    let exprs = parse_exprs("a * a == 4");
    assert!(matches!(
        translate(&exprs, &bounds),
        Err(TranslationError::Unsupported(_))
    ));
    // Unknown identifier.
    let exprs = parse_exprs("zz == 1");
    assert!(matches!(
        translate(&exprs, &bounds),
        Err(TranslationError::Unsupported(_))
    ));
}

#[test]
fn fallback_backend_reports_unsupported() {
    let exprs = parse_exprs("a == 5");
    let problem = translate(&exprs, &bounds_from(&[("a", (0, 10))])).expect("translatable");
    // Without the solver feature the backend defers to rejection sampling;
    // with it the assignment must satisfy the constraint.
    match solve(&problem, 1) {
        SolveOutcome::Solved(values) => {
            assert_eq!(values, vec![5]);
        }
        SolveOutcome::Unsupported => {}
        other => panic!("unexpected outcome: {other:?}"),
    }
}

#[cfg(feature = "solver-pumpkin")]
mod pumpkin {
    use super::*;

    fn solved_values(source: &str, bounds: BTreeMap<String, (i64, i64)>, seed: u64) -> Vec<i128> {
        let exprs = parse_exprs(source);
        let problem = translate(&exprs, &bounds).expect("translatable");
        match solve(&problem, seed) {
            SolveOutcome::Solved(values) => values,
            other => panic!("expected a solution, got {other:?}"),
        }
    }

    #[test]
    fn reports_unsat_for_non_integer_diophantine() {
        // a + b == 100 and b == 2a ⟹ 3a == 100: no integer solution.
        // Rejection sampling could never establish this; the solver does.
        let exprs = parse_exprs("a + b == 100\nb == 2 * a");
        let problem = translate(&exprs, &bounds_from(&[("a", (0, 100)), ("b", (0, 100))]))
            .expect("translatable");
        assert_eq!(solve(&problem, 1), SolveOutcome::Unsat);
    }

    #[test]
    fn solves_soluble_conjunction_and_is_deterministic() {
        let source = "a + b == 100\na >= 40";
        let bounds = bounds_from(&[("a", (0, 100)), ("b", (0, 100))]);
        let first = solved_values(source, bounds.clone(), 1);
        let second = solved_values(source, bounds, 1);
        assert_eq!(first, second, "same seed must be deterministic");
        assert!(
            first[0] + first[1] == 100,
            "a + b == 100 violated: {first:?}"
        );
        assert!(first[0] >= 40, "a >= 40 violated: {first:?}");
    }

    #[test]
    fn solves_coefficients() {
        let values = solved_values(
            "2 * a - b == 21",
            bounds_from(&[("a", (0, 100)), ("b", (0, 100))]),
            7,
        );
        assert_eq!(2 * values[0] - values[1], 21);
    }

    #[test]
    fn reports_unsatisfiable() {
        let exprs = parse_exprs("a > 100");
        let problem = translate(&exprs, &bounds_from(&[("a", (0, 15))])).expect("translatable");
        assert_eq!(solve(&problem, 1), SolveOutcome::Unsat);
    }
}
