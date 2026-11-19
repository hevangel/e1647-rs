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
        context: None,
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
    // 12.2.6: a sub-expression's own sampling event overrides the
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
    // starting after the previous succeeded (12.2.9).
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
    // A yield whose lhs never succeeds holds vacuously (12.2.14: the
    // rhs is not evaluated when the lhs fails).
    let vacuous = [State::new(&["other"]), State::new(&["other"])];
    assert!(holds_tightly(&golden, &vacuous, &callbacks).unwrap());
    // `not @a` holds where a does not occur; `fail @a` holds where a
    // can no longer occur (12.2.17 vs 12.2.13).
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
    // proposition (12.2.11, 12.2.4).
    let te = parse_temporal_expression("~[1..3]*true(sig_on) @q").expect("parses");
    let path = [State::new(&["q"]), State::new(&["q"]), State::new(&["q"])];
    assert!(succeeds(&te, &path, &callbacks).unwrap());
    let short = [State::new(&["q"])];
    assert!(succeeds(&te, &short, &callbacks).unwrap());
    // `[2]*cycle @q` — a fixed repeat of two cycles (12.2.10).
    let fixed = parse_temporal_expression("[2]*cycle @q").expect("parses");
    let two = [State::new(&["q"]), State::new(&["q"])];
    assert!(succeeds(&fixed, &two, &callbacks).unwrap());
    assert!(!succeeds(&fixed, &short, &callbacks).unwrap());
    // `eventually @b` holds when b occurs anywhere on the path
    // (12.2.15).
    let eventually = parse_temporal_expression("eventually @b").expect("parses");
    let late = [State::new(&["a"]), State::new(&["a"]), State::new(&["b"])];
    assert!(succeeds(&eventually, &late, &callbacks).unwrap());
    // First-match repeats live in sequences (12.2.12's equivalence:
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

#[test]
fn recorded_propositions_survive_rescans_and_monitor_clones() {
    let te = parse_temporal_expression("{true(flag); true(not flag)}").unwrap();
    let mut propositions = Vec::new();
    let mut bounds = Vec::new();
    e1647::temporal::collect_exprs(&te, &mut propositions, &mut bounds);
    let mut states = [State::default(), State::default()];
    for (index, state) in states.iter_mut().enumerate() {
        state.record_boolean(7, propositions[0], Some(index == 0));
        state.record_boolean(7, propositions[1], Some(index == 1));
        state.record_boolean(8, propositions[0], Some(false));
        state.record_boolean(8, propositions[1], Some(false));
    }
    let mut cb = EvalCallbacks {
        context: Some(7),
        boolean: Box::new(|_| Some(false)),
        numeric: Box::new(|_| None),
    };
    assert!(holds_tightly(&te, &states, &cb).unwrap());
    let mut monitor = e1647::temporal::RuleMonitor::new(&te);
    assert!(!monitor.observe(&states[..1], &cb).unwrap());
    assert!(!monitor.observe(&states, &cb).unwrap());
    assert!(!monitor.failed_at_end());
    cb.context = Some(8);
    assert!(!holds_tightly(&te, &states, &cb).unwrap());
    cb.context = Some(9);
    cb.boolean = Box::new(|_| Some(true));
    assert!(
        !holds_tightly(&te, &states, &cb).unwrap(),
        "missing history must not use current values"
    );
}

#[test]
fn recorded_repeat_bound_uses_the_starting_state() {
    let te = parse_temporal_expression("[n]*cycle").unwrap();
    let mut propositions = Vec::new();
    let mut bounds = Vec::new();
    e1647::temporal::collect_exprs(&te, &mut propositions, &mut bounds);
    let mut states = [State::default(), State::default()];
    states[0].record_numeric(1, bounds[0], Some(2));
    states[1].record_numeric(1, bounds[0], Some(9));
    let cb = EvalCallbacks {
        context: Some(1),
        boolean: Box::new(|_| None),
        numeric: Box::new(|_| Some(9)),
    };
    assert!(holds_tightly(&te, &states, &cb).unwrap());
}

#[test]
fn scalar_edges_compare_numeric_magnitude_and_require_a_baseline() {
    use e1647::ast::EdgeRoutine::{Change, Fall, Rise};
    use e1647::temporal::sampled_edge;
    for routine in [Change, Rise, Fall] {
        assert!(!sampled_edge(routine, None, 7));
        assert!(!sampled_edge(routine, Some(7), 7));
    }
    assert!(sampled_edge(Rise, Some(7), 8));
    assert!(!sampled_edge(Fall, Some(7), 8));
    assert!(sampled_edge(Fall, Some(-3), -9));
    assert!(sampled_edge(Change, Some(1), 0));
    assert!(sampled_edge(Rise, Some(0), 1));
    assert!(sampled_edge(Fall, Some(1), 0));
}

#[test]
fn recorded_edges_survive_monitor_cloning_and_context_isolation() {
    use e1647::ast::EdgeRoutine::Rise;
    let te = parse_temporal_expression("rise(flag)").unwrap();
    let TemporalExpr::EdgeSample { expr, .. } = &te else {
        panic!("edge expected");
    };
    let mut state = State::default();
    state.record_edge(1, Rise, expr, Some(true));
    state.record_edge(2, Rise, expr, Some(false));
    let mut cb = EvalCallbacks {
        context: Some(1),
        boolean: Box::new(|_| None),
        numeric: Box::new(|_| None),
    };
    assert!(succeeds(&te, &[state.clone()], &cb).unwrap());
    let mut monitor = e1647::temporal::RuleMonitor::new(&te);
    monitor.observe(&[state.clone()], &cb).unwrap();
    assert!(!monitor.failed_at_end());
    cb.context = Some(2);
    assert!(!succeeds(&te, &[state.clone()], &cb).unwrap());
    cb.context = Some(3);
    assert_eq!(
        succeeds(&te, &[state], &cb),
        Err(EvalError::EdgeSample("rise"))
    );
}

#[test]
fn edge_sampling_events_override_the_enclosing_event() {
    let te = parse_temporal_expression("{rise(a) @qa; fall(b) @qb} @outer").unwrap();
    let mut edges = Vec::new();
    e1647::temporal::collect_edges(&te, None, &mut edges);
    assert_eq!(edges.len(), 2);
    assert_eq!(edges[0].2, Some(&event("qa")));
    assert_eq!(edges[1].2, Some(&event("qb")));
}

#[test]
fn monitor_fires_an_invariant_when_the_forbidden_event_occurs() {
    let te = parse_temporal_expression("not @bad").expect("parses");
    let empty_b: std::collections::BTreeMap<String, bool> = std::collections::BTreeMap::new();
    let empty_n: std::collections::BTreeMap<String, i128> = std::collections::BTreeMap::new();
    let cb = callbacks(&empty_b, &empty_n);
    let mut monitor = e1647::temporal::RuleMonitor::new(&te);
    // Three quiet states, then the forbidden occurrence.
    let log = [
        State::new(&[]),
        State::new(&[]),
        State::new(&[]),
        State::new(&["bad"]),
        State::new(&[]),
    ];
    // Observing the prefix without the occurrence: not yet violated.
    assert!(!monitor.observe(&log[..3], &cb).expect("evaluates"));
    // The occurrence state: certain now.
    assert!(monitor.observe(&log[..4], &cb).expect("evaluates"));
    assert!(monitor.failed_at_end());
}

#[test]
fn monitor_matches_rule_failed_over_random_logs() {
    // Property test: for a deterministic sample of temporal expressions
    // and pseudo-random logs, the streaming monitor's end-of-run
    // verdict equals the whole-log `rule_failed` verdict — and its
    // incremental consumption (mixed one- and two-state steps) must not
    // change the verdict.
    let tes: Vec<TemporalExpr> = [
        "@a => eventually @b",
        "@a => @b",
        "not @a",
        "fail @a",
        "eventually @a",
        "{@a; @b}",
        "@a and @b",
        "@a or @b",
        "{[2]*cycle; @b}",
        "not (@a and @b)",
        "@a => {[1]*cycle; @b} or @c",
    ]
    .iter()
    .map(|text| parse_temporal_expression(text).expect("parses"))
    .collect();
    let empty_b: std::collections::BTreeMap<String, bool> = std::collections::BTreeMap::new();
    let empty_n: std::collections::BTreeMap<String, i128> = std::collections::BTreeMap::new();
    let cb = callbacks(&empty_b, &empty_n);
    // A splitmix64-style LCG for reproducible logs.
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = move || {
        state = state.wrapping_add(0xa076_1d64_7886_0a4d);
        let z = state;
        (z ^ (z >> 30).wrapping_mul(0xbf58_476d_1ce4_e5b9)) >> 11
    };
    for _case in 0..200 {
        let length = (next() % 18) as usize + 1;
        let log: Vec<State> = (0..length)
            .map(|_| {
                let pick = next() % 8;
                match pick {
                    0 => State::new(&["a"]),
                    1 => State::new(&["b"]),
                    2 => State::new(&["c"]),
                    3 => State::new(&["a", "b"]),
                    4 => State::new(&["b", "c"]),
                    _ => State::new(&[]),
                }
            })
            .collect();
        for te in &tes {
            let expected = e1647::temporal::rule_failed(te, &log, &cb).expect("evaluates");
            let mut monitor = e1647::temporal::RuleMonitor::new(te);
            let mut fed = 0usize;
            while fed < log.len() {
                let step = if next() % 2 == 0 { 1 } else { 2 };
                let end = (fed + step).min(log.len());
                monitor.observe(&log[..end], &cb).expect("evaluates");
                fed = end;
            }
            let actual = monitor.failed_at_end();
            assert_eq!(
                actual, expected,
                "monitor verdict diverged for {te:?} over log {log:?}"
            );
        }
    }
}
