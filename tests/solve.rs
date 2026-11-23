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
        "struct t {\n    a: uint(bits: 8);\n    b: int;\n    c: byte;\n    d: nibble;\n    e: bool;\n    f: list of uint;\n    g: uint(bits: 40);\n    h: int(bits: 40);\n};\n",
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
    assert_eq!(fields[6], ("g".to_string(), None));
    assert_eq!(fields[7], ("h".to_string(), None));
}

#[test]
fn solver_translation_declines_overflowing_coefficients_and_constants() {
    let bounds = bounds_from(&[("a", (0, 10)), ("b", (0, 10))]);
    for source in [
        "a + 9223372036854775807 + 1 == 0",
        "9223372036854775807 * a + a == 0",
        "9223372036854775807 * a == -a",
        "a + 9223372036854775807 == -2",
        "9223372036854775807 * a * a == -a * a",
        "(3037000499 * a + 3037000499 * b) * (3037000499 * a + 3037000499 * b) == 0",
        "-(-9223372036854775807 * a * a - a * a) == 0",
    ] {
        assert!(
            matches!(
                translate(&parse_exprs(source), &bounds),
                Err(TranslationError::Unsupported(_))
            ),
            "{source}"
        );
    }
}

#[test]
fn enumeration_handles_minimum_rhs_and_declines_full_width_domains() {
    let exprs = parse_exprs("a * a >= (-9223372036854775807 - 1)");
    let small = translate(&exprs, &bounds_from(&[("a", (0, 2))])).unwrap();
    for seed in 0..6 {
        assert_eq!(
            solve(&small, seed),
            SolveOutcome::Solved(vec![i128::from(seed % 3)])
        );
    }

    let exprs = parse_exprs("a * a == 0");
    let mut wide = translate(&exprs, &bounds_from(&[("a", (0, 2))])).unwrap();
    wide.vars[0].lower = i64::MIN;
    wide.vars[0].upper = i64::MAX;
    assert_eq!(solve(&wide, 1), SolveOutcome::Unsupported);
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
fn translate_item_membership_into_disjunction() {
    // `a in [1, 5, 9]` is the disjunction of the equalities; each item
    // goes through the same affine translation as a comparison side.
    let exprs = parse_exprs("a in [1, 5, 9]");
    let problem = translate(&exprs, &bounds_from(&[("a", (0, 255))])).expect("translatable");
    let e1647::solve::BoolNode::Any(branches) = &problem.constraints[0] else {
        panic!("expected the item list to translate as a disjunction");
    };
    assert_eq!(branches.len(), 3);
    let expected_rhs = [1, 5, 9];
    for (branch, rhs) in branches.iter().zip(expected_rhs) {
        let e1647::solve::BoolNode::Atom(linear) = branch else {
            panic!("expected an equality atom");
        };
        assert_eq!(linear.terms, vec![("a".to_string(), 1)]);
        assert_eq!(linear.rhs, rhs);
        assert_eq!(linear.cmp, e1647::solve::Cmp::Eq);
    }
}

#[test]
fn translate_disjunction_into_any() {
    let exprs = parse_exprs("a < 3 || a > 7");
    let problem = translate(&exprs, &bounds_from(&[("a", (0, 9))])).expect("translatable");
    let e1647::solve::BoolNode::Any(branches) = &problem.constraints[0] else {
        panic!("expected the disjunction to translate as Any");
    };
    assert_eq!(branches.len(), 2);
    let e1647::solve::BoolNode::Atom(low) = &branches[0] else {
        panic!("expected the first branch atom");
    };
    assert_eq!(low.cmp, e1647::solve::Cmp::Lt);
    assert_eq!(low.rhs, 3);
    let e1647::solve::BoolNode::Atom(high) = &branches[1] else {
        panic!("expected the second branch atom");
    };
    assert_eq!(high.cmp, e1647::solve::Cmp::Gt);
    assert_eq!(high.rhs, 7);
}

#[test]
fn translate_negated_membership_de_morgan() {
    // `a not in [1..5]` parses as the unary `not` over the range
    // membership: ¬(a >= 1 ∧ a <= 5) = a < 1 ∨ a > 5.
    let exprs = parse_exprs("a not in [1..5]");
    let problem = translate(&exprs, &bounds_from(&[("a", (0, 9))])).expect("translatable");
    let e1647::solve::BoolNode::Any(branches) = &problem.constraints[0] else {
        panic!("expected the negated range to translate as a disjunction");
    };
    assert_eq!(branches.len(), 2);
    let e1647::solve::BoolNode::Atom(below) = &branches[0] else {
        panic!("expected the below-range atom");
    };
    assert_eq!(below.cmp, e1647::solve::Cmp::Lt);
    assert_eq!(below.rhs, 1);
    let e1647::solve::BoolNode::Atom(above) = &branches[1] else {
        panic!("expected the above-range atom");
    };
    assert_eq!(above.cmp, e1647::solve::Cmp::Gt);
    assert_eq!(above.rhs, 5);
}

#[test]
fn translate_negated_conjunction_swaps_the_connective() {
    // ¬(a < 1 ∧ b < 2) = a >= 1 ∨ b >= 2 (De Morgan).
    let exprs = parse_exprs("not (a < 1 && b < 2)");
    let problem =
        translate(&exprs, &bounds_from(&[("a", (0, 9)), ("b", (0, 9))])).expect("translatable");
    let e1647::solve::BoolNode::Any(branches) = &problem.constraints[0] else {
        panic!("expected the negated conjunction to translate as a disjunction");
    };
    assert_eq!(branches.len(), 2);
    let e1647::solve::BoolNode::Atom(a) = &branches[0] else {
        panic!("expected the first negated atom");
    };
    assert_eq!(
        a.terms,
        vec![("a".to_string(), 1)],
        "negation must not move the comparison's sides"
    );
    assert_eq!(a.cmp, e1647::solve::Cmp::Ge);
    assert_eq!(a.rhs, 1);
    let e1647::solve::BoolNode::Atom(b) = &branches[1] else {
        panic!("expected the second negated atom");
    };
    assert_eq!(b.cmp, e1647::solve::Cmp::Ge);
    assert_eq!(b.rhs, 2);
}

#[test]
fn translate_double_negation_restores_the_atom() {
    let exprs = parse_exprs("not not (a == 4)");
    let problem = translate(&exprs, &bounds_from(&[("a", (0, 9))])).expect("translatable");
    let e1647::solve::BoolNode::Atom(linear) = &problem.constraints[0] else {
        panic!("expected the doubly negated atom restored");
    };
    assert_eq!(linear.cmp, e1647::solve::Cmp::Eq);
    assert_eq!(linear.rhs, 4);
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
    for source in [
        "a / a == 4",
        "a / 0 == 4",
        "a / 2 == -1",
        "a / -5 == 2",
        "a % 0 == 1",
    ] {
        assert!(matches!(
            translate(&parse_exprs(source), &bounds),
            Err(TranslationError::Unsupported(_))
        ));
    }
    let signed_boundary = bounds_from(&[("a", (i64::from(i32::MIN), -1))]);
    assert!(translate(&parse_exprs("a / -1 == 0"), &signed_boundary).is_err());
    // Unknown identifier.
    let exprs = parse_exprs("zz == 1");
    assert!(matches!(
        translate(&exprs, &bounds),
        Err(TranslationError::Unsupported(_))
    ));
    // A disjunct the affine translation cannot express (here a bare
    // identifier, which is not a comparison).
    let exprs = parse_exprs("a == 1 || zz");
    assert_eq!(
        translate(&exprs, &bounds),
        Err(TranslationError::Unsupported(
            "constraint is not a comparison"
        ))
    );
}

#[test]
fn translates_remainders_within_the_branch_budget() {
    use e1647::solve::{BoolNode, Cmp, Linear};

    let bounds = bounds_from(&[("a", (7, 19))]);
    let expected = vec![BoolNode::Any(
        [8, 13, 18]
            .into_iter()
            .map(|rhs| {
                BoolNode::Atom(Linear {
                    terms: vec![("a".to_owned(), 1)],
                    rhs,
                    cmp: Cmp::Eq,
                })
            })
            .collect(),
    )];
    assert_eq!(
        translate(&parse_exprs("3 == a % 5"), &bounds)
            .unwrap()
            .constraints,
        expected
    );
    let boundary = translate(&parse_exprs("a % 2 == 1"), &bounds_from(&[("a", (0, 127))])).unwrap();
    assert!(matches!(&boundary.constraints[..], [BoolNode::Any(branches)] if branches.len() == 64));
    let wider = translate(&parse_exprs("a % 2 == 1"), &bounds_from(&[("a", (0, 129))])).unwrap();
    assert!(matches!(
        &wider.constraints[..],
        [BoolNode::Remainder {
            divisor: 2,
            remainder: 1,
            ..
        }]
    ));
    let impossible = translate(&parse_exprs("a % 5 == 5"), &bounds).unwrap();
    assert_eq!(solve(&impossible, 0), SolveOutcome::Unsat);
}

#[test]
fn enumerates_a_large_remainder_with_a_polynomial_constraint() {
    let bounds = bounds_from(&[("a", (0, 200))]);
    for (source, expected) in [
        ("a % 2 == 1 && a * a == 9", 3),
        ("a % 2 != 0 && a * a == 9", 3),
        ("not (a % 2 == 1) && a * a == 16", 4),
    ] {
        let problem = translate(&parse_exprs(source), &bounds).unwrap();
        assert_eq!(solve(&problem, 7), SolveOutcome::Solved(vec![expected]));
    }
}

#[test]
fn quotient_comparisons_match_truncating_integer_division() {
    let bounds = bounds_from(&[("a", (17, 33))]);
    for (comparison, low, high) in [
        ("a / 10 < 2", 17, 19),
        ("2 > a / 10", 17, 19),
        ("a / 10 <= 2", 17, 29),
        ("2 >= a / 10", 17, 29),
        ("a / 10 > 2", 30, 33),
        ("2 < a / 10", 30, 33),
        ("a / 10 >= 2", 20, 33),
        ("2 <= a / 10", 20, 33),
        ("a / 10 <= 2147483647", 17, 33),
    ] {
        let source = format!("{comparison} && a * a >= 0");
        let problem = translate(&parse_exprs(&source), &bounds).unwrap();
        for (seed, value) in (low..=high).chain([low]).enumerate() {
            assert_eq!(
                solve(&problem, seed as u64),
                SolveOutcome::Solved(vec![value]),
                "{comparison}, seed {seed}"
            );
        }
    }
    for source in ["a / 10 < 0", "a / 10 > 2147483647"] {
        let impossible = translate(&parse_exprs(source), &bounds).unwrap();
        assert_eq!(solve(&impossible, 0), SolveOutcome::Unsat);
    }
}

#[test]
fn ordered_remainder_comparisons_match_integer_remainders() {
    let bounds = bounds_from(&[("a", (7, 17))]);
    for (source, expected) in [
        ("a % 5 < 2", vec![10, 11, 15, 16]),
        ("2 >= a % 5", vec![7, 10, 11, 12, 15, 16, 17]),
        ("a % 5 > 2", vec![8, 9, 13, 14]),
        ("2 <= a % 5", vec![7, 8, 9, 12, 13, 14, 17]),
        ("not (a % 5 < 2)", vec![7, 8, 9, 12, 13, 14, 17]),
    ] {
        let source = format!("({source}) && a * a >= 0");
        let problem = translate(&parse_exprs(&source), &bounds).unwrap();
        for (seed, value) in expected.iter().copied().chain([expected[0]]).enumerate() {
            assert_eq!(
                solve(&problem, seed as u64),
                SolveOutcome::Solved(vec![value])
            );
        }
    }
    for source in ["a % 5 < 0", "a % 5 >= 5"] {
        let impossible = translate(&parse_exprs(source), &bounds).unwrap();
        assert_eq!(solve(&impossible, 0), SolveOutcome::Unsat);
    }
}

#[test]
fn signed_remainder_matches_truncation_toward_zero() {
    let bounds = bounds_from(&[("a", (-11, 11))]);
    for (constraint, expected) in [
        ("a % 5 == -2", vec![-7, -2]),
        ("a % 5 == 0", vec![-10, -5, 0, 5, 10]),
        ("-4 >= a % 5", vec![-9, -4]),
        ("a % 5 > 3", vec![4, 9]),
        ("a % -5 == -2", vec![-7, -2]),
        ("a % (-5) == 2", vec![2, 7]),
    ] {
        let source = format!("{constraint} && a * a >= 0");
        let problem = translate(&parse_exprs(&source), &bounds).unwrap();
        for (seed, value) in expected.iter().copied().chain([expected[0]]).enumerate() {
            assert_eq!(
                solve(&problem, seed as u64),
                SolveOutcome::Solved(vec![value])
            );
        }
    }
    let impossible = translate(&parse_exprs("a % 5 == -5"), &bounds).unwrap();
    assert_eq!(solve(&impossible, 0), SolveOutcome::Unsat);
}

#[test]
fn translates_nonnegative_quotient_equalities_as_ranges() {
    use e1647::solve::{BoolNode, Cmp, Linear};

    let bounds = bounds_from(&[("a", (0, 25))]);
    let expected = vec![BoolNode::All(vec![
        BoolNode::Atom(Linear {
            terms: vec![("a".to_owned(), 1)],
            rhs: 20,
            cmp: Cmp::Ge,
        }),
        BoolNode::Atom(Linear {
            terms: vec![("a".to_owned(), 1)],
            rhs: 25,
            cmp: Cmp::Le,
        }),
    ])];
    for source in ["a / 10 == 2", "2 == a / 10"] {
        assert_eq!(
            translate(&parse_exprs(source), &bounds)
                .unwrap()
                .constraints,
            expected
        );
    }
    let impossible = translate(&parse_exprs("a / 10 == 3"), &bounds).unwrap();
    assert_eq!(solve(&impossible, 0), SolveOutcome::Unsat);
}

#[test]
fn signed_quotients_truncate_toward_zero() {
    let bounds = bounds_from(&[("a", (-15, 15))]);
    for (comparison, low, high) in [
        ("a / 5 == -2", -14, -10),
        ("-2 == a / 5", -14, -10),
        ("a / 5 == 0", -4, 4),
        ("a / 5 == 2", 10, 14),
        ("a / 5 < -2", -15, -15),
        ("-2 >= a / 5", -15, -10),
        ("a / 5 > -2", -9, 15),
        ("a / 5 >= -2", -14, 15),
        ("(a / (-5)) == 2", -14, -10),
        ("-2 == a / -5", 10, 14),
        ("a / -5 <= -2", 10, 15),
        ("a / -5 < 2", -9, 15),
        ("-2 >= a / -5", 10, 15),
    ] {
        let source = format!("{comparison} && a * a >= 0");
        let problem = translate(&parse_exprs(&source), &bounds).unwrap();
        for (seed, value) in (low..=high).chain([low]).enumerate() {
            assert_eq!(
                solve(&problem, seed as u64),
                SolveOutcome::Solved(vec![value]),
                "{comparison}, seed {seed}"
            );
        }
    }
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

// --- Non-linear arithmetic: the polynomial IR and the enumeration solver ---

#[test]
fn linear_products_stay_linear() {
    // A constant factor is affine: the backend posts it directly.
    let exprs = parse_exprs("2 * a == 10");
    let problem = translate(&exprs, &bounds_from(&[("a", (0, 50))])).expect("translatable");
    assert!(matches!(
        problem.constraints.first(),
        Some(e1647::solve::BoolNode::Atom(_))
    ));
}

#[test]
fn variable_products_translate_into_polynomials() {
    let exprs = parse_exprs("a * b == 100");
    let problem =
        translate(&exprs, &bounds_from(&[("a", (0, 50)), ("b", (0, 50))])).expect("translatable");
    let Some(e1647::solve::BoolNode::Poly(polynomial)) = problem.constraints.first() else {
        panic!("expected a polynomial atom");
    };
    assert_eq!(
        polynomial.terms,
        vec![(
            {
                let mut monomial = std::collections::BTreeMap::new();
                monomial.insert("a".to_string(), 1);
                monomial.insert("b".to_string(), 1);
                monomial
            },
            1
        )]
    );
    assert_eq!(polynomial.rhs, 100);
    assert_eq!(polynomial.cmp, e1647::solve::Cmp::Eq);
}

#[test]
fn polynomial_expansion_spreads_the_product() {
    // (a + 1) * (b - 2) == 0  ⟺  a·b - 2a + b == 2
    let exprs = parse_exprs("(a + 1) * (b - 2) == 0");
    let problem =
        translate(&exprs, &bounds_from(&[("a", (0, 9)), ("b", (0, 9))])).expect("translatable");
    let Some(e1647::solve::BoolNode::Poly(polynomial)) = problem.constraints.first() else {
        panic!("expected a polynomial atom");
    };
    let monomial = |names: &[(&str, u32)]| -> std::collections::BTreeMap<String, u32> {
        names
            .iter()
            .map(|(name, exponent)| (name.to_string(), *exponent))
            .collect()
    };
    assert_eq!(
        polynomial.terms,
        vec![
            (monomial(&[("a", 1)]), -2),
            (monomial(&[("a", 1), ("b", 1)]), 1),
            (monomial(&[("b", 1)]), 1),
        ]
    );
    assert_eq!(polynomial.rhs, 2);
}

#[test]
fn enumeration_solves_a_square_deterministically() {
    // a² == 2500 has the unique solution a == 50 over 0..255: every seed
    // picks it.
    let exprs = parse_exprs("a * a == 2500");
    for seed in [1u64, 7, 42] {
        let problem = translate(&exprs, &bounds_from(&[("a", (0, 255))])).expect("translatable");
        assert_eq!(solve(&problem, seed), SolveOutcome::Solved(vec![50]));
    }
}

#[test]
fn enumeration_reports_unsat_for_an_impossible_square() {
    // 2501 sits strictly between 50² and 51²: no integer solution.
    let exprs = parse_exprs("a * a == 2501");
    let problem = translate(&exprs, &bounds_from(&[("a", (0, 255))])).expect("translatable");
    assert_eq!(solve(&problem, 1), SolveOutcome::Unsat);
}

#[test]
fn enumeration_handles_mixed_constraint_trees() {
    // a·b == 6 with a < b over 1..6: exactly (1, 6) and (2, 3) satisfy
    // the tree; every seed's pick must satisfy both constraints.
    let exprs = parse_exprs("a * b == 6\na < b");
    let bounds = bounds_from(&[("a", (1, 6)), ("b", (1, 6))]);
    let problem = translate(&exprs, &bounds).expect("translatable");
    for seed in [1u64, 2, 3, 99] {
        match solve(&problem, seed) {
            SolveOutcome::Solved(values) => {
                let (a, b) = (values[0], values[1]);
                assert_eq!(a * b, 6, "seed {seed}: {values:?}");
                assert!(a < b, "seed {seed}: {values:?}");
            }
            other => panic!("seed {seed}: expected a solution, got {other:?}"),
        }
    }
}

#[test]
fn negated_nonlinear_constraint_enumerates() {
    // ¬(a² == 4) over 0..9: every pick must avoid 2 (the domain is
    // unsigned, so -2 is not a member anyway).
    let exprs = parse_exprs("not (a * a == 4)");
    let problem = translate(&exprs, &bounds_from(&[("a", (0, 9))])).expect("translatable");
    for seed in [1u64, 5, 13] {
        match solve(&problem, seed) {
            SolveOutcome::Solved(values) => {
                assert_ne!(values[0], 2, "seed {seed}: {values:?}");
            }
            other => panic!("seed {seed}: expected a solution, got {other:?}"),
        }
    }
}

#[test]
fn enumeration_defers_wide_domains() {
    // 2001 × 2001 candidate tuples exceed the 2^20 enumeration cap:
    // the outcome defers to the rejection sampler.
    let exprs = parse_exprs("a * b == 100");
    let problem = translate(&exprs, &bounds_from(&[("a", (0, 2000)), ("b", (0, 2000))]))
        .expect("translatable");
    assert_eq!(solve(&problem, 1), SolveOutcome::Unsupported);
}

#[test]
fn polynomial_degree_above_the_cap_is_unsupported() {
    let exprs = parse_exprs("a * a * a * a * a == 1");
    let error = translate(&exprs, &bounds_from(&[("a", (0, 9))]))
        .expect_err("a fifth power is above the degree cap");
    assert!(matches!(error, TranslationError::Unsupported(_)));
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

    #[test]
    fn solves_disjunction_and_is_deterministic() {
        let source = "a == 1 || a == 2";
        let bounds = bounds_from(&[("a", (0, 15))]);
        let first = solved_values(source, bounds.clone(), 3);
        let second = solved_values(source, bounds, 3);
        assert_eq!(first, second, "same seed must pick the same branch");
        assert!(
            first == vec![1] || first == vec![2],
            "the assignment must satisfy one branch: {first:?}"
        );
    }

    #[test]
    fn solves_nonnegative_quotient_equality() {
        let values = solved_values("a / 10 == 2", bounds_from(&[("a", (0, 100))]), 7);
        assert!((20..=29).contains(&values[0]));
    }

    #[test]
    fn solves_ordered_quotient_constraints_with_pumpkin() {
        let values = solved_values(
            "2 <= a / 10 && a / 10 < 3 && a > 28",
            bounds_from(&[("a", (0, 100))]),
            7,
        );
        assert_eq!(values, vec![29]);
    }

    #[test]
    fn solves_signed_quotient_constraints_with_pumpkin() {
        let values = solved_values(
            "a / 5 == -2 && a < -13",
            bounds_from(&[("a", (-20, 20))]),
            7,
        );
        assert_eq!(values, vec![-14]);
        let values = solved_values(
            "a / -5 == 2 && a < -13",
            bounds_from(&[("a", (-20, 20))]),
            7,
        );
        assert_eq!(values, vec![-14]);
    }

    #[test]
    fn solves_remainder_equality_with_other_constraints() {
        let values = solved_values("a % 5 == 3 && a > 14", bounds_from(&[("a", (7, 19))]), 1);
        assert_eq!(values, vec![18]);
    }

    #[test]
    fn solves_large_remainder_without_expanding_value_branches() {
        let values = solved_values(
            "a % 7 == 3 && a >= 6003 && a <= 6010",
            bounds_from(&[("a", (5000, 20000))]),
            1,
        );
        assert_eq!(values, vec![6009]);
    }

    #[test]
    fn excludes_a_large_remainder_without_expanding_value_branches() {
        let bounds = bounds_from(&[("a", (5000, 20000))]);
        for source in [
            "a % 7 != 3 && a >= 6009 && a <= 6010",
            "not (a % 7 == 3) && a >= 6009 && a <= 6010",
        ] {
            assert_eq!(solved_values(source, bounds.clone(), 1), vec![6010]);
        }
        let impossible = translate(&parse_exprs("a % 1 != 0"), &bounds).unwrap();
        assert_eq!(solve(&impossible, 1), SolveOutcome::Unsat);
    }

    #[test]
    fn solves_ordered_remainders_without_expanding_value_branches() {
        let values = solved_values(
            "a % 7 < 4 && a % 7 >= 3 && a >= 6008 && a <= 6011",
            bounds_from(&[("a", (5000, 20000))]),
            1,
        );
        assert_eq!(values, vec![6009]);
    }

    #[test]
    fn solves_signed_remainders_with_pumpkin() {
        let bounds = bounds_from(&[("a", (-20000, 20000))]);
        let values = solved_values("a % 5 == -2 && a <= -6001 && a >= -6003", bounds.clone(), 7);
        assert_eq!(values, vec![-6002]);
        let impossible = translate(&parse_exprs("a % 5 == 3 && a == -2"), &bounds).unwrap();
        assert_eq!(solve(&impossible, 0), SolveOutcome::Unsat);
    }

    #[test]
    fn solves_nested_disjunctions_and_uses_the_full_seed() {
        let source = "(a == 1 || (a == 2 && (b == 3 || b == 4)) || a == 3) && b == 4";
        let bounds = bounds_from(&[("a", (0, 15)), ("b", (0, 15))]);
        assert_eq!(solved_values(source, bounds.clone(), 0), vec![1, 4]);
        assert_eq!(solved_values(source, bounds, 1 << 32), vec![2, 4]);
    }

    #[test]
    fn nested_branch_union_respects_the_cap() {
        let source = format!(
            "a == 1 && ({})",
            std::iter::repeat_n("a == 1", 65)
                .collect::<Vec<_>>()
                .join(" || ")
        );
        let problem = translate(&parse_exprs(&source), &bounds_from(&[("a", (0, 15))])).unwrap();
        assert_eq!(solve(&problem, 1), SolveOutcome::Unsupported);
    }

    #[test]
    fn solves_item_membership() {
        let values = solved_values("a in [1, 5, 9]", bounds_from(&[("a", (0, 15))]), 7);
        assert!(
            values == vec![1] || values == vec![5] || values == vec![9],
            "the assignment must be one of the items: {values:?}"
        );
    }

    #[test]
    fn solves_disjunction_over_variables() {
        // `a in [b, b + 1]` — item expressions go through the affine
        // translation, so the disjunction stays linear.
        let values = solved_values(
            "a in [b, b + 1]",
            bounds_from(&[("a", (0, 50)), ("b", (0, 50))]),
            7,
        );
        let delta = values[0] - values[1];
        assert!(
            delta == 0 || delta == 1,
            "a must equal b or b + 1: {values:?}"
        );
    }

    #[test]
    fn reports_unsat_when_every_branch_fails() {
        let exprs = parse_exprs("a < 0 || a > 15");
        let problem = translate(&exprs, &bounds_from(&[("a", (0, 15))])).expect("translatable");
        assert_eq!(solve(&problem, 1), SolveOutcome::Unsat);
    }

    #[test]
    fn combinations_beyond_the_cap_fall_back_to_unsupported() {
        // Seven separate two-branch disjunctions multiply to 2^7 = 128
        // branch combinations — beyond MAX_BRANCH_COMBINATIONS — so the
        // outcome defers to the rejection sampler rather than exploding.
        // (A chained `||` flattens to N alternatives, not 2^N; the cap is
        // about the product of independent disjunctions.)
        let source = "a == 1 || a == 2\na == 1 || a == 2\na == 1 || a == 2\na == 1 || a == 2\na == 1 || a == 2\na == 1 || a == 2\na == 1 || a == 2";
        let exprs = parse_exprs(source);
        let problem = translate(&exprs, &bounds_from(&[("a", (0, 15))])).expect("translatable");
        assert_eq!(solve(&problem, 1), SolveOutcome::Unsupported);
    }
}
