use e1647::ast::{RepeatKind, TemporalExpr};
use e1647::parser::parse_temporal_expression;
use e1647::temporal::{
    EvalCallbacks, EvalError, State, holds_tightly, sampled_normal_form, succeeds,
};

/// Proposition/numeric callbacks keyed by the rendered expression text,
/// enough for tests without the interpreter.
fn callbacks<'a>(
    booleans: &'a std::collections::BTreeMap<String, bool>,
    numerics: &'a std::collections::BTreeMap<String, i128>,
) -> EvalCallbacks<'a> {
    EvalCallbacks {
        boolean: Box::new(|expr| match expr {
            e1647::ast::Expr::Ident { name, .. } => booleans.get(name).copied(),
            _ => None,
        }),
        numeric: Box::new(|expr| match expr {
            e1647::ast::Expr::IntLiteral { text, .. } => text.parse::<i128>().ok(),
            e1647::ast::Expr::Ident { name, .. } => numerics.get(name).copied(),
            _ => None,
        }),
    }
}

fn event(name: &str) -> TemporalExpr {
    TemporalExpr::EventRef {
        path: name.split('.').map(|segment| segment.to_owned()).collect(),
    }
}

#[test]
fn propagation_matches_the_clause_worked_example() {
    // 12.1.4's example: `{@a; ~[..]*cycle; @b} @q` propagates q into
    // every atom: `{@a @q; ~[..]*cycle @q; @b @q} @q`.
    let te = parse_temporal_expression("{@a; ~[..]*cycle; @b} @q").expect("parses");
    let q = event("q");
    let normal = sampled_normal_form(&te, &q);
    let TemporalExpr::Sampled { te: inner, event } = &normal else {
        panic!("expected the outer sampling kept: {normal:?}");
    };
    assert!(matches!(&**event, TemporalExpr::EventRef { .. }));
    let TemporalExpr::Sequence { items } = &**inner else {
        panic!("expected the sequence: {normal:?}");
    };
    assert_eq!(items.len(), 3);
    for item in items {
        let TemporalExpr::Sampled { te: atom, event } = item else {
            panic!("expected each item sampled with q: {item:?}");
        };
        assert!(matches!(&**event, TemporalExpr::EventRef { .. }));
        // The first and last items are event atoms; the middle is the
        // unbounded repeat of cycle.
        assert!(matches!(
            &**atom,
            TemporalExpr::EventRef { .. } | TemporalExpr::Repeat { .. }
        ));
    }
    // The repeat item is sampled as a unit — `~[..]*cycle @q` — and its
    // interior keeps the bare `cycle` of the clause's printed example.
    let TemporalExpr::Sampled {
        te: repeat_item, ..
    } = &items[1]
    else {
        panic!("expected the sampled repeat item");
    };
    let TemporalExpr::Repeat { te: repeated, .. } = &**repeat_item else {
        panic!("expected the repeat: {repeat_item:?}");
    };
    assert!(matches!(&**repeated, TemporalExpr::Cycle));
}

#[test]
fn explicit_sub_sampling_overrides_the_context() {
    // 12.2.5: a sub-expression's own sampling event overrides the
    // context for everything below it.
    let te = parse_temporal_expression("{@a @p; @b} @q").expect("parses");
    let q = event("q");
    let normal = sampled_normal_form(&te, &q);
    let TemporalExpr::Sampled { te: inner, .. } = &normal else {
        panic!("expected the outer sampling: {normal:?}");
    };
    let TemporalExpr::Sequence { items } = &**inner else {
        panic!("expected the sequence: {normal:?}");
    };
    // The first item keeps p as its sampling event.
    let TemporalExpr::Sampled { event, .. } = &items[0] else {
        panic!("expected the sampled first item");
    };
    let p_path = match &**event {
        TemporalExpr::EventRef { path } => path.join("."),
        _ => panic!("expected an event path"),
    };
    assert_eq!(p_path, "p");
}

#[test]
fn event_atoms_cycles_and_sequences_hold_over_paths() {
    let empty_b: std::collections::BTreeMap<String, bool> = std::collections::BTreeMap::new();
    let empty_n: std::collections::BTreeMap<String, i128> = std::collections::BTreeMap::new();
    let callbacks = callbacks(&empty_b, &empty_n);
    // `@a` holds on the single state where a occurs.
    let te = parse_temporal_expression("@a").expect("parses");
    assert!(succeeds(&te, &[State::new(&["a"])], &callbacks).unwrap());
    assert!(!succeeds(&te, &[State::new(&["b"])], &callbacks).unwrap());
    // A sequence holds over consecutive sampling periods, each item
    // starting after the previous succeeded (12.2.8).
    let seq = parse_temporal_expression("{@a; @b; @c}").expect("parses");
    let path = [State::new(&["a"]), State::new(&["b"]), State::new(&["c"])];
    assert!(succeeds(&seq, &path, &callbacks).unwrap());
    // Wrong order does not.
    let scrambled = [State::new(&["b"]), State::new(&["a"]), State::new(&["c"])];
    assert!(!succeeds(&seq, &scrambled, &callbacks).unwrap());
    // The trailing event must occur strictly after the previous item.
    let glued = [State::new(&["a", "b"]), State::new(&["c"])];
    assert!(!succeeds(&seq, &glued, &callbacks).unwrap());
}

#[test]
fn cycle_and_sampling_hold() {
    // 12.2's illustration: `cycle @q` holds tightly on a path where q
    // occurs in the last state and in no other — tight satisfaction
    // (12.1.1.2) considers the path as a whole. Success (12.1.1.12) is
    // the weaker last-point notion: on a two-cycle path, the final
    // cycle alone is a sub-path over which the TE holds tightly.
    let te = parse_temporal_expression("cycle @q").expect("parses");
    let empty_b: std::collections::BTreeMap<String, bool> = std::collections::BTreeMap::new();
    let empty_n: std::collections::BTreeMap<String, i128> = std::collections::BTreeMap::new();
    let callbacks = callbacks(&empty_b, &empty_n);
    let single = [State::new(&["q"])];
    let two = [State::new(&["q"]), State::new(&["q"])];
    assert!(holds_tightly(&te, &single, &callbacks).unwrap());
    assert!(!holds_tightly(&te, &two, &callbacks).unwrap());
    assert!(succeeds(&te, &two, &callbacks).unwrap());
}

#[test]
fn yield_not_and_fail_are_distinguishable() {
    let empty_b: std::collections::BTreeMap<String, bool> = std::collections::BTreeMap::new();
    let empty_n: std::collections::BTreeMap<String, i128> = std::collections::BTreeMap::new();
    let callbacks = callbacks(&empty_b, &empty_n);
    // The golden's shape: `@enter => {@fetch} @clk` — the expect-style
    // check is tight satisfaction over the monitoring window
    // (12.1.1.2): with the fetch occurring after enter, the yield
    // holds; without fetch, it fails.
    let golden =
        parse_temporal_expression("@enter_exec_st => {@fetch1_assert} @clk").expect("parses");
    let good = [
        State::new(&["enter_exec_st"]),
        State::new(&["fetch1_assert"]),
    ];
    assert!(holds_tightly(&golden, &good, &callbacks).unwrap());
    let missing = [State::new(&["enter_exec_st"]), State::new(&["other"])];
    assert!(!holds_tightly(&golden, &missing, &callbacks).unwrap());
    // A yield whose lhs never succeeds holds vacuously (12.2.13: the
    // rhs is not evaluated when the lhs fails).
    let vacuous = [State::new(&["other"]), State::new(&["other"])];
    assert!(holds_tightly(&golden, &vacuous, &callbacks).unwrap());
    // `not @a` holds where a does not occur; `fail @a` holds where a
    // can no longer occur (12.2.16 vs 12.2.12).
    let not_a = parse_temporal_expression("not @a").expect("parses");
    let fail_a = parse_temporal_expression("fail @a").expect("parses");
    let a_absent = [State::new(&["b"]), State::new(&["b"])];
    assert!(succeeds(&not_a, &a_absent, &callbacks).unwrap());
    assert!(succeeds(&fail_a, &a_absent, &callbacks).unwrap());
    let a_present = [State::new(&["a"])];
    assert!(!succeeds(&not_a, &a_present, &callbacks).unwrap());
    assert!(!succeeds(&fail_a, &a_present, &callbacks).unwrap());
}

#[test]
fn repeats_props_and_eventually_evaluate() {
    let mut booleans = std::collections::BTreeMap::new();
    booleans.insert("sig_on".to_owned(), true);
    let mut numerics = std::collections::BTreeMap::new();
    numerics.insert("two".to_owned(), 2);
    let callbacks = callbacks(&booleans, &numerics);
    // `~[1..3]*true(sig_on) @q` — a true-match repeat over a
    // proposition (12.2.10, 12.2.3).
    let te = parse_temporal_expression("~[1..3]*true(sig_on) @q").expect("parses");
    let path = [State::new(&["q"]), State::new(&["q"]), State::new(&["q"])];
    assert!(succeeds(&te, &path, &callbacks).unwrap());
    let short = [State::new(&["q"])];
    assert!(succeeds(&te, &short, &callbacks).unwrap());
    // `[2]*cycle @q` — a fixed repeat of two cycles (12.2.9).
    let fixed = parse_temporal_expression("[2]*cycle @q").expect("parses");
    let two = [State::new(&["q"]), State::new(&["q"])];
    assert!(succeeds(&fixed, &two, &callbacks).unwrap());
    assert!(!succeeds(&fixed, &short, &callbacks).unwrap());
    // `eventually @b` holds when b occurs anywhere on the path
    // (12.2.14).
    let eventually = parse_temporal_expression("eventually @b").expect("parses");
    let late = [State::new(&["a"]), State::new(&["a"]), State::new(&["b"])];
    assert!(succeeds(&eventually, &late, &callbacks).unwrap());
    // First-match repeats live in sequences (12.2.11's equivalence:
    // {[..]*@a; @b}).
    let first_match = parse_temporal_expression("{[..]*@a; @b}").expect("parses");
    let wait_then_b = [State::new(&["a"]), State::new(&["a"]), State::new(&["b"])];
    assert!(succeeds(&first_match, &wait_then_b, &callbacks).unwrap());
}

#[test]
fn edge_samples_report_the_value_history_gap() {
    // `change(exp)` needs the sampled value history across states —
    // not available in the pure core; the error is honest.
    let te = parse_temporal_expression("change(sig) @q").expect("parses");
    let empty_b: std::collections::BTreeMap<String, bool> = std::collections::BTreeMap::new();
    let empty_n: std::collections::BTreeMap<String, i128> = std::collections::BTreeMap::new();
    let callbacks = callbacks(&empty_b, &empty_n);
    let result = succeeds(&te, &[State::new(&["q"])], &callbacks);
    assert_eq!(result, Err(EvalError::EdgeSample("change")));
}

#[test]
fn repeat_bounds_must_evaluate() {
    let empty_b: std::collections::BTreeMap<String, bool> = std::collections::BTreeMap::new();
    let empty_n: std::collections::BTreeMap<String, i128> = std::collections::BTreeMap::new();
    let callbacks = callbacks(&empty_b, &empty_n);
    // `[n]*cycle` with an unresolvable n reports the gap.
    let te = parse_temporal_expression("[n]*cycle @q").expect("parses");
    let result = succeeds(&te, &[State::new(&["q"]), State::new(&["q"])], &callbacks);
    assert_eq!(
        result,
        Err(EvalError::Unsupported(
            "repeat bounds must evaluate to numeric values"
        ))
    );
    // The repeat kind is recorded even when evaluation cannot.
    let TemporalExpr::Sampled { te: inner, .. } = &te else {
        panic!("expected the sampling");
    };
    let TemporalExpr::Repeat { kind, .. } = &**inner else {
        panic!("expected the repeat");
    };
    assert_eq!(*kind, RepeatKind::Fixed);
}
