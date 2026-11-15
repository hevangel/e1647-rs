use e1647::elab::elaborate;
use e1647::interp::{RunResult, run};
use e1647::parser::parse;

fn run_source(source: &str, seed: u64) -> RunResult {
    let parsed = parse(source);
    assert!(
        parsed.diagnostics.is_empty(),
        "parse diagnostics: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
    let elaborated = elaborate(&parsed.declarations);
    assert!(
        elaborated.diagnostics.is_empty(),
        "elaboration diagnostics: {:?}",
        elaborated
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
    run(&elaborated, seed)
}

/// Coverage tests explicitly enable collection during setup; ordinary
/// execution tests retain the language's initially disabled mode.
fn run_source_with_coverage(source: &str, seed: u64) -> RunResult {
    run_source(
        &format!(
            "{source}\nextend sys {{ setup() is also {{ set_config(cover, mode, normal); }}; }};"
        ),
        seed,
    )
}

/// Like [`run_source`], but the `hdl_*` routines and the `@sim`/event-port
/// sampling read the injected per-cycle signal history instead of a
/// spawned simulator (the `xezim` binary is not available in tests).
fn run_source_with_signals(
    source: &str,
    seed: u64,
    signals: e1647::hdl::InjectedSignals,
) -> RunResult {
    let parsed = parse(source);
    assert!(
        parsed.diagnostics.is_empty(),
        "parse diagnostics: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
    let elaborated = elaborate(&parsed.declarations);
    assert!(
        elaborated.diagnostics.is_empty(),
        "elaboration diagnostics: {:?}",
        elaborated
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
    e1647::interp::run_with_signals(&elaborated, seed, signals)
}

fn clean_output(source: &str, seed: u64) -> String {
    let result = run_source(source, seed);
    assert!(
        result.diagnostics.is_empty(),
        "run diagnostics: {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
    result.output
}

#[test]
fn sys_run_executes_print_and_out() {
    let output = clean_output(
        "extend sys {\n    run() is also {\n        var x: uint = 6 * 7;\n        print x;\n        out(\"answer=\", x, \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "x = 42\nanswer=42\n");
}

#[test]
fn code_block_delimiters_wrap_a_program() {
    let output = clean_output(
        "<'\nextend sys {\n    run() is also {\n        out(\"in a code block\\n\");\n    };\n};\n'>\n",
        1,
    );
    assert_eq!(output, "in a code block\n");
}

#[test]
fn if_while_break_and_continue_execute() {
    let output = clean_output(
        "extend sys {\n    run() is also {\n        var i: uint = 0;\n        while i < 10 {\n            i = i + 1;\n            if i == 2 { continue; };\n            if i == 5 { break; };\n        };\n        print i;\n    };\n};\n",
        1,
    );
    assert_eq!(output, "i = 5\n");
}

#[test]
fn method_calls_params_and_returns_execute() {
    let output = clean_output(
        "struct math_s {\n    double_it(v: uint): uint is {\n        return v * 2;\n    };\n    apply(): uint is {\n        return self.double_it(21);\n    };\n};\nextend sys {\n    run() is also {\n        var m: math_s;\n        print m.apply();\n    };\n};\n",
        1,
    );
    assert_eq!(output, "42\n");
}

#[test]
fn based_and_prefixed_literals_evaluate() {
    let output = clean_output(
        "extend sys {\n    run() is also {\n        out(8'hFF, \" \", 'h10, \" \", 0b1010, \" \", 0o17, \" \", 8'b1010_1010, \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "255 16 10 15 170\n");
}

#[test]
fn multipliers_and_character_literals_evaluate() {
    let output = clean_output(
        "extend sys {\n    run() is also {\n        out(32K, \" \", 2m, \" \", 1_0K, \" \", 0c\"a\", \" \", 0c\"\\n\", \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "32768 2097152 10240 97 10\n");
}

#[test]
fn mvl_literals_are_reported_not_evaluated() {
    let result = run_source(
        "extend sys {\n    run() is also {\n        print 8'b10x1;\n    };\n};\n",
        1,
    );
    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>(),
        vec!["MVL literal '8'b10x1' is not evaluated yet".to_string()]
    );
}

#[test]
fn lists_support_push_size_and_indexing() {
    let output = clean_output(
        "extend sys {\n    run() is also {\n        var xs: list of uint;\n        xs.push(3);\n        xs.push(4);\n        print xs.size(), xs[0] + xs[1];\n        xs[1] = 10;\n        print xs[1];\n    };\n};\n",
        1,
    );
    assert_eq!(output, "2 7\n10\n");
}

// --- Clause 27 list pseudo-methods ---

/// The rendered diagnostics of a run expected to fail.
fn run_error(source: &str) -> Vec<String> {
    let result = run_source(source, 1);
    assert!(
        !result.diagnostics.is_empty(),
        "expected a diagnostic for: {source}"
    );
    result
        .diagnostics
        .iter()
        .map(|d| d.render())
        .collect::<Vec<_>>()
}

#[test]
fn list_pop_pop0_remove_and_return_the_ends() {
    // 27.3.10 pop(): removes and returns the last item. 27.3.11 pop0():
    // removes and returns the first item, renumbering the rest.
    let output = clean_output(
        "extend sys {\n    run() is also {\n        var xs: list of int;\n        \
         xs.push(1); xs.push(2); xs.push(3);\n        \
         print xs.pop();\n        print xs;\n        print xs.pop0();\n        print xs;\n    };\n};\n",
        1,
    );
    assert_eq!(output, "3\nxs = [1, 2]\n1\nxs = [2]\n");
}

#[test]
fn list_top_top0_read_without_removing() {
    // 27.4.26 top(): the last item, not removed (pairs with pop).
    // 27.4.27 top0(): the first item, not removed (pairs with pop0 for
    // queues, per the prose's note).
    let output = clean_output(
        "extend sys {\n    run() is also {\n        var xs: list of int;\n        \
         xs.push(1); xs.push(2); xs.push(3);\n        \
         print xs.top(), xs.top0(), xs.size();\n    };\n};\n",
        1,
    );
    assert_eq!(output, "3 1 3\n");
}

#[test]
fn list_push0_insert_place_items_by_position() {
    // 27.3.13 push0(item): the item becomes index 0. 27.3.8
    // insert(index, item): index == size appends (the prose's stated
    // case), earlier positions shift the rest back.
    let output = clean_output(
        "extend sys {\n    run() is also {\n        var xs: list of int;\n        \
         xs.push(1); xs.push(2); xs.push(3);\n        \
         xs.push0(9);\n        print xs;\n        \
         xs.insert(2, 5);\n        print xs;\n        \
         xs.insert(xs.size(), 7);\n        print xs;\n    };\n};\n",
        1,
    );
    assert_eq!(
        output,
        "xs = [9, 1, 2, 3]\nxs = [9, 1, 5, 2, 3]\nxs = [9, 1, 5, 2, 3, 7]\n"
    );
}

#[test]
fn list_delete_fast_delete_remove_items() {
    // 27.3.6 delete(index): removes the item at the index, renumbering
    // sequentially. 27.3.7 fast_delete(index): the last item's value
    // moves into the deleted slot, so the other indexes stay.
    let output = clean_output(
        "extend sys {\n    run() is also {\n        var xs: list of int;\n        \
         xs.push(1); xs.push(2); xs.push(3); xs.push(4);\n        \
         xs.fast_delete(0);\n        print xs;\n        \
         xs.delete(1);\n        print xs;\n    };\n};\n",
        1,
    );
    assert_eq!(output, "xs = [4, 2, 3]\nxs = [4, 3]\n");
}

#[test]
fn list_clear_and_is_empty() {
    // 27.3.5 clear(): deletes all items. 27.4.12 is_empty(): TRUE only
    // when the list has no items.
    let output = clean_output(
        "extend sys {\n    run() is also {\n        var xs: list of int;\n        \
         print xs.is_empty();\n        \
         xs.push(1);\n        print xs.is_empty();\n        \
         xs.clear();\n        print xs.is_empty(), xs.size();\n    };\n};\n",
        1,
    );
    assert_eq!(output, "TRUE\nFALSE\nTRUE 0\n");
}

#[test]
fn list_push_push0_add_add0_of_a_list_extend() {
    // 27.3.2 add(list) / 27.3.14 push(list): a copy of the argument list
    // is appended. 27.3.4 add0(list) / 27.3.15 push0(list): it is placed
    // at the head. e declares no lists of lists (27.4.25), so a
    // list-valued argument is unambiguously the list form.
    let output = clean_output(
        "extend sys {\n    run() is also {\n        var a: list of int;\n        \
         var b: list of int;\n        a.push(1); a.push(2);\n        b.push(7); b.push(8);\n        \
         a.add0(b);\n        print a;\n        \
         a.push(b);\n        print a;\n    };\n};\n",
        1,
    );
    assert_eq!(output, "a = [7, 8, 1, 2]\na = [7, 8, 1, 2, 7, 8]\n");
}

#[test]
fn list_reverse_copy_and_exists() {
    // 27.4.21 reverse(): a new list in reverse order, receiver unchanged.
    // 27.4.3 copy(): a new list; modifying the copy leaves the original.
    // 27.4.5 exists(): TRUE only for an index number that exists.
    let output = clean_output(
        "extend sys {\n    run() is also {\n        var xs: list of int;\n        \
         xs.push(1); xs.push(2); xs.push(3);\n        \
         print xs.reverse();\n        print xs;\n        \
         var ys: list of int = xs.copy();\n        ys.delete(0);\n        \
         print xs.size(), ys.size();\n        \
         print xs.exists(0), xs.exists(2), xs.exists(3);\n    };\n};\n",
        1,
    );
    assert_eq!(output, "[3, 2, 1]\nxs = [1, 2, 3]\n3 2\nTRUE TRUE FALSE\n");
}

#[test]
fn list_min_max_prefer_the_latest_on_ties() {
    // 27.4.15 max()/27.4.18 min(): the item with the largest/smallest
    // value. 27.4.16 max_index()/27.4.19 min_index(): the same scan as an
    // index — and on ties the latest item wins, which the index forms
    // make observable.
    let output = clean_output(
        "extend sys {\n    run() is also {\n        var xs: list of int;\n        \
         xs.push(3); xs.push(1); xs.push(4); xs.push(1); xs.push(5);\n        \
         print xs.max(), xs.min();\n        \
         var ys: list of int;\n        ys.push(5); ys.push(1); ys.push(5);\n        \
         print ys.max_index(), ys.min_index();\n        \
         var ws: list of string;\n        ws.push(\"b\"); ws.push(\"a\");\n        \
         print ws.max(), ws.min();\n    };\n};\n",
        1,
    );
    assert_eq!(output, "5 1\n2 1\nb a\n");
}

#[test]
fn list_sum_product_and_all_or_all_with_empty_results() {
    // 27.5.5 sum(): the integer sum, 0 when empty. 27.5.4 product(): the
    // integer product, 1 when empty. 27.5.1 and_all(): TRUE when empty.
    // 27.5.2 or_all(): FALSE when empty. The no-argument forms read the
    // items themselves; the exp-parameter forms are a documented gap.
    let output = clean_output(
        "extend sys {\n    run() is also {\n        var xs: list of int;\n        \
         xs.push(2); xs.push(3); xs.push(4);\n        \
         print xs.sum(), xs.product();\n        \
         var none_yet: list of int;\n        print none_yet.sum(), none_yet.product();\n        \
         var bs: list of bool;\n        bs.push(TRUE); bs.push(FALSE);\n        \
         print bs.and_all(), bs.or_all();\n        \
         var none: list of bool;\n        print none.and_all(), none.or_all();\n    };\n};\n",
        1,
    );
    assert_eq!(output, "9 24\n0 1\nFALSE TRUE\nTRUE FALSE\n");
}

#[test]
fn list_is_a_permutation_ignores_order() {
    // 27.4.11 is_a_permutation(): TRUE when the argument holds the same
    // items with the same multiplicities, in any order.
    let output = clean_output(
        "extend sys {\n    run() is also {\n        var a: list of int;\n        \
         a.push(1); a.push(2); a.push(2);\n        \
         var b: list of int;\n        b.push(2); b.push(1); b.push(2);\n        \
         print a.is_a_permutation(b);\n        \
         var short: list of int;\n        short.push(2); short.push(1);\n        \
         print a.is_a_permutation(short);\n        \
         var other: list of int;\n        other.push(1); other.push(3); other.push(2);\n        \
         print a.is_a_permutation(other);\n    };\n};\n",
        1,
    );
    assert_eq!(output, "TRUE\nFALSE\nFALSE\n");
}

#[test]
fn list_modifiers_work_through_struct_fields() {
    // The pseudo-methods attach to any list expression (27.2) — a struct
    // field receiver writes back through the field.
    let output = clean_output(
        "struct queue_s {\n    items: list of int;\n};\n\
         extend sys {\n    run() is also {\n        var q: queue_s;\n        \
         q.items.push(1); q.items.push(2); q.items.push(3);\n        \
         print q.items.pop();\n        print q.items.size();\n        \
         q.items.insert(0, 9);\n        print q.items;\n    };\n};\n",
        1,
    );
    assert_eq!(output, "3\n2\n[9, 1, 2]\n");
}

// --- Clause 4.11.4 list identity (`list_ptr==` / `!list_ptr=`) ---

#[test]
fn clause_4_11_4_list_ptr_compares_object_identity() {
    // The migration plan's runtime test for `list_ptr==` / `!list_ptr=`:
    // `.copy()` mints a distinct list object, assignment aliases the same
    // object, and the regular `==` stays item-by-item.
    let output = clean_output(
        "<'\nextend sys {\n    run() is also {\n        var l1: list of int = {1; 2; 3};\n        \
         var l2: list of int = l1.copy();\n        var l3: list of int = l1;\n        \
         out(l1 list_ptr== l2, \"\\n\");\n        out(l1 list_ptr== l3, \"\\n\");\n        \
         out(l1 !list_ptr= l2, \"\\n\");\n        out(l1 !list_ptr= l3, \"\\n\");\n        \
         out(l1 == l2, \"\\n\");\n    };\n};\n'>\n",
        1,
    );
    assert_eq!(output, "FALSE\nTRUE\nTRUE\nFALSE\nTRUE\n");
}

#[test]
fn clause_4_11_4_regular_equality_stays_item_by_item() {
    // Two independently built lists with equal contents: `==` is TRUE,
    // `list_ptr==` is FALSE (distinct objects), and length/item
    // differences make `==` FALSE.
    let output = clean_output(
        "extend sys {\n    run() is also {\n        var a: list of int = {1; 2};\n        \
         var b: list of int = {1; 2};\n        var c: list of int = {1; 2; 3};\n        \
         out(a == b, \" \", a == c, \" \", a list_ptr== b, \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "TRUE FALSE FALSE\n");
}

#[test]
fn clause_4_11_4_list_assignment_aliases_and_copy_isolates() {
    // The shared list object: mutations through one alias are visible
    // through the other; a `.copy()` result is isolated.
    let output = clean_output(
        "extend sys {\n    run() is also {\n        var l1: list of int = {1; 2};\n        \
         var alias: list of int = l1;\n        var l2: list of int = l1.copy();\n        \
         l1.push(3);\n        out(l1, \" \", alias, \" \", l2, \"\\n\");\n        \
         alias.push0(0);\n        out(l1, \"\\n\");\n        \
         l2.push(9);\n        out(l1, \" \", l2, \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(
        output,
        "[1, 2, 3] [1, 2, 3] [1, 2]\n[0, 1, 2, 3]\n[0, 1, 2, 3] [1, 2, 9]\n"
    );
}

#[test]
fn clause_4_11_4_list_ptr_requires_lists_of_the_same_type() {
    // 4.11.4: both operands must be lists of the same type. A mismatched
    // declared element type (`list of int` vs `list of uint`) and a
    // non-list operand both report `E0411` at run time.
    let diagnostics = run_error(
        "extend sys {\n    run() is also {\n        var a: list of int;\n        \
         var b: list of uint;\n        out(a list_ptr== b);\n    };\n};\n",
    );
    assert!(
        diagnostics[0].contains("E0411") && diagnostics[0].contains("lists of the same type"),
        "{diagnostics:?}"
    );
    let diagnostics = run_error(
        "extend sys {\n    run() is also {\n        var a: list of int;\n        \
         out(a list_ptr== 3);\n    };\n};\n",
    );
    assert!(diagnostics[0].contains("E0411"), "{diagnostics:?}");
}

#[test]
fn clause_4_6_list_concat_builds_a_fresh_list() {
    // `{ e1; e2; ... }` (clause 4.6, Table 12): a list element
    // contributes its items, a scalar element contributes itself.
    let output = clean_output(
        "extend sys {\n    run() is also {\n        var l1: list of int = {1; 2};\n        \
         var l2: list of int = {3};\n        var joined: list of int = {l1; 9; l2};\n        \
         out(joined, \" \", joined.size(), \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "[1, 2, 9, 3] 4\n");
}

#[test]
fn empty_list_accessors_report_e0402() {
    // 27.3.10/27.3.11 and 27.4.26/27.4.27: "If the list is empty, an
    // error shall be issued." 27.4.15/27.4.18: "If the list is empty, an
    // error shall be issued." Reported like the set pseudo-methods'
    // empty-set errors (E0402).
    for (call, name) in [
        ("pop()", "pop"),
        ("pop0()", "pop0"),
        ("top()", "top"),
        ("top0()", "top0"),
        ("max()", "max"),
        ("min()", "min"),
    ] {
        let source = format!(
            "extend sys {{\n    run() is also {{\n        var xs: list of int;\n        print xs.{call};\n    }};\n}};\n"
        );
        let diagnostics = run_error(&source);
        assert_eq!(diagnostics.len(), 1, "{name}: {diagnostics:?}");
        assert!(
            diagnostics[0].contains("E0402") && diagnostics[0].contains("empty list"),
            "{name}: {diagnostics:?}"
        );
    }
}

#[test]
fn list_index_pseudo_methods_report_e0404() {
    // 27.3.6/27.3.7: an index that does not exist in the list issues an
    // error; 27.3.8: a size smaller than index issues an error. E0404 is
    // the list index out-of-range code (the same one the [i] expression
    // reports).
    let cases = [
        "xs.delete(3);",
        "xs.fast_delete(3);",
        "xs.insert(3, 9);",
        "xs.delete(-1);",
    ];
    for call in cases {
        let source = format!(
            "extend sys {{\n    run() is also {{\n        var xs: list of int;\n        \
             xs.push(1); xs.push(2);\n        {call}\n    }};\n}};\n"
        );
        let diagnostics = run_error(&source);
        assert_eq!(diagnostics.len(), 1, "{call}: {diagnostics:?}");
        assert!(
            diagnostics[0].contains("E0404") && diagnostics[0].contains("out of range"),
            "{call}: {diagnostics:?}"
        );
    }
}

#[test]
fn list_aggregates_reject_wrong_item_types() {
    // 27.5.4/27.5.5 define integer results; 27.5.1/27.5.2 read boolean
    // items; the 27.4.15-27.4.19 extremes order items the way the
    // relational operators do. Anything else is a type error.
    let cases = [
        (
            "var xs: list of string; xs.push(\"a\"); print xs.sum();",
            "sum()",
        ),
        (
            "var xs: list of string; xs.push(\"a\"); print xs.product();",
            "product()",
        ),
        (
            "var xs: list of int; xs.push(1); print xs.and_all();",
            "and_all()",
        ),
        (
            "var xs: list of int; xs.push(1); print xs.or_all();",
            "or_all()",
        ),
        (
            "var xs: list of bool; xs.push(TRUE); xs.push(FALSE); print xs.max();",
            "max()",
        ),
    ];
    for (source, name) in cases {
        let diagnostics = run_error(&format!(
            "extend sys {{\n    run() is also {{\n        {source}\n    }};\n}};\n"
        ));
        assert_eq!(diagnostics.len(), 1, "{name}: {diagnostics:?}");
        assert!(diagnostics[0].contains("E0402"), "{name}: {diagnostics:?}");
    }
}

#[test]
fn unsupported_list_methods_report_e0405() {
    // The expression-taking pseudo-methods (apply 27.4.2, sort 27.4.23 —
    // documented gaps) and unknown names stay E0405.
    for call in ["xs.apply(1);", "xs.sort();", "xs.foo();"] {
        let source = format!(
            "extend sys {{\n    run() is also {{\n        var xs: list of int;\n        \
             xs.push(1);\n        {call}\n    }};\n}};\n"
        );
        let diagnostics = run_error(&source);
        assert_eq!(diagnostics.len(), 1, "{call}: {diagnostics:?}");
        assert!(
            diagnostics[0].contains("E0405") && diagnostics[0].contains("not supported"),
            "{call}: {diagnostics:?}"
        );
    }
}

#[test]
fn gen_respects_keep_constraints_and_is_deterministic() {
    let source = "struct pixel_s {\n    intensity: uint(bits: 8);\n    keep intensity < 128;\n};\nextend sys {\n    run() is also {\n        var p: pixel_s;\n        gen p;\n        print p.intensity;\n    };\n};\n";
    let first = clean_output(source, 1);
    let second = clean_output(source, 1);
    assert_eq!(first, second, "the same seed must reproduce the run");
    let value: i128 = first.trim().parse().expect("integer output");
    assert!((0..128).contains(&value), "constraint violated: {value}");
}

#[test]
fn gen_enforces_a_disjunctive_keep_constraint() {
    // The solver seam accepts disjunctions (branch enumeration): with the
    // solver feature a satisfying branch is solved; without it the
    // rejection sampler enforces the constraint by evaluation. Either
    // way every value must satisfy the disjunction, both branches must
    // stay reachable, and one seed must reproduce its run.
    let source = "struct gated_s {\n    a: uint(bits: 8);\n    keep a == 1 or a == 200;\n};\nextend sys {\n    run() is also {\n        var p: gated_s;\n        gen p;\n        print p.a;\n    };\n};\n";
    assert_eq!(clean_output(source, 4), clean_output(source, 4));
    let mut seen = std::collections::BTreeSet::new();
    for seed in 1..=40 {
        let output = clean_output(source, seed);
        let value: i128 = output.trim().parse().expect("integer output");
        assert!(
            value == 1 || value == 200,
            "seed {seed} violated the disjunction: {value}"
        );
        seen.insert(value);
    }
    assert!(
        seen.contains(&1) && seen.contains(&200),
        "both branches must stay reachable: {seen:?}"
    );
}

#[test]
fn gen_enforces_negated_and_item_set_constraints() {
    // `not in [lo..hi]` and item-list membership translate into the
    // solver's disjunction IR; under the rejection sampler they are
    // enforced by evaluation. The sampled values must satisfy both.
    let source = "struct gated_s {\n    a: uint(bits: 8);\n    keep a not in [10..90];\n    keep a in [1, 100, 200];\n};\nextend sys {\n    run() is also {\n        var p: gated_s;\n        gen p;\n        print p.a;\n    };\n};\n";
    let mut seen = std::collections::BTreeSet::new();
    for seed in 1..=40 {
        let output = clean_output(source, seed);
        let value: i128 = output.trim().parse().expect("integer output");
        assert!(
            value == 1 || value == 100 || value == 200,
            "seed {seed} left the item set: {value}"
        );
        seen.insert(value);
    }
    assert!(
        seen.len() >= 2,
        "the disjunctive domains must stay reachable: {seen:?}"
    );
}

#[test]
fn gen_solves_a_nonlinear_constraint_by_enumeration() {
    // a² == 2500 over 0..255 has the unique solution 50; the bounded
    // enumeration solver (feature-independent) finds it in every build
    // and every seed, where the rejection sampler used to give up after
    // 1000 attempts.
    let source = "struct square_s {\n    a: uint(bits: 8);\n    keep a * a == 2500;\n};\nextend sys {\n    run() is also {\n        var p: square_s;\n        gen p;\n        print p.a;\n    };\n};\n";
    for seed in [1u64, 7, 123] {
        let output = clean_output(source, seed);
        assert_eq!(output.trim(), "50", "seed {seed}");
    }
}

#[test]
fn gen_solves_a_product_constraint_and_is_deterministic() {
    // a·b == 6 with a < b: the seeded enumeration pick satisfies both
    // constraints and reproduces per seed.
    let source = "struct pair_s {\n    a: uint(bits: 4);\n    b: uint(bits: 4);\n    keep a * b == 6;\n    keep a < b;\n};\nextend sys {\n    run() is also {\n        var p: pair_s;\n        gen p;\n        print p.a * p.b, p.a < p.b;\n    };\n};\n";
    let first = clean_output(source, 4);
    let second = clean_output(source, 4);
    assert_eq!(first, second, "the same seed must reproduce the run");
    // `print` renders its arguments space-separated on one line.
    let fields: Vec<&str> = first.split_whitespace().collect();
    assert_eq!(fields, vec!["6", "TRUE"], "output: {first}");
}

#[test]
fn gen_of_struct_recurses_into_list_elements() {
    let output = clean_output(
        "struct pixel_s {\n    intensity: uint(bits: 8);\n};\nstruct frame_s {\n    pixels: list of pixel_s;\n    keep pixels.size() > 0;\n};\nextend sys {\n    run() is also {\n        var f: frame_s;\n        gen f;\n        print f.pixels[0].intensity;\n    };\n};\n",
        1,
    );
    // The frame's list satisfied `size() > 0`, so element 0 exists and was
    // itself randomized; the value is seed-deterministic (the determinism
    // test above covers that).
    let value: i128 = output.trim().parse().expect("integer output");
    assert!((0..256).contains(&value));
}

#[test]
fn unsatisfiable_constraints_report_e0407() {
    let result = run_source(
        "struct impossible_s {\n    a: uint(bits: 4);\n    keep a > 100;\n};\nextend sys {\n    run() is also {\n        var p: impossible_s;\n        gen p;\n    };\n};\n",
        1,
    );
    let codes: Vec<&str> = result
        .diagnostics
        .iter()
        .map(|d| d.code)
        .collect::<Vec<_>>();
    assert_eq!(codes, vec!["E0407"]);
}

#[test]
fn declarations_only_program_runs_to_completion_with_no_output() {
    let result = run_source(
        "struct packet { addr: uint(bits: 8); keep addr < 128; };\n",
        1,
    );
    assert!(result.diagnostics.is_empty());
    assert_eq!(result.output, "");
}

#[test]
fn division_by_zero_and_unknown_names_are_diagnostics() {
    let result = run_source(
        "extend sys {\n    run() is also {\n        var x: uint = 1 / 0;\n    };\n};\n",
        1,
    );
    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec!["E0403"]
    );
    let result = run_source(
        "extend sys {\n    run() is also {\n        print nope;\n    };\n};\n",
        1,
    );
    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec!["E0401"]
    );
}

#[test]
fn enumeral_fields_and_comparisons_execute() {
    let output = clean_output(
        "type mode_e: [IDLE, BUSY];\nstruct channel_s {\n    mode: mode_e;\n};\nextend sys {\n    run() is also {\n        var c: channel_s;\n        gen c;\n        if c.mode == BUSY {\n            out(\"busy\\n\");\n        } else {\n            out(\"idle\\n\");\n        };\n    };\n};\n",
        1,
    );
    assert!(output == "busy\n" || output == "idle\n", "output: {output}");
}

#[test]
fn hdl_path_access_without_a_session_is_e0408() {
    // Quoted HDL paths parse (asic-world driver style) but require a
    // simulator session started with hdl_load.
    let result = run_source(
        "extend sys {\n    run() is also {\n        'tb.sig' = 1;\n        print 'tb.sig';\n    };\n};\n",
        1,
    );
    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec!["E0408"]
    );
}

#[test]
fn case_ternary_and_outf_execute() {
    let output = clean_output(
        "extend sys {\n    run() is also {\n        var x: uint = 2;\n        case x {\n            1: { out(\"one\\n\"); };\n            [2, 7]: { out(\"two-or-seven\\n\"); };\n            default: { out(\"other\\n\"); };\n        };\n        var verdict: string = x > 1 ? \"big\" : \"small\";\n        out(verdict, \"\\n\");\n        outf(\"hex=%x dec=%d str=%s 100%%\\n\", 255, 42, \"tag\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "two-or-seven\nbig\nhex=ff dec=42 str=tag 100%\n");
}

#[test]
fn compute_calls_tcm_inline_and_waits_advance() {
    let output = clean_output(
        "unit counter_u {\n    ticks: uint;\n    tick() @sys.any is {\n        var i: uint = 0;\n        while i < 3 {\n            i = i + 1;\n            ticks = ticks + 1;\n            wait cycle;\n        };\n    };\n};\nextend sys {\n    c: counter_u is instance;\n    run() is also {\n        compute c.tick();\n        print c.ticks;\n        emit sys.done;\n    };\n};\n",
        1,
    );
    // The compute-called TCM runs atomically to its return; its waits
    // advance the cycles inline.
    assert!(output.contains("3\n"), "output: {output}");
}

#[test]
fn start_schedules_the_tcm_and_the_starter_continues() {
    // 13.2.2: `start` enqueues the TCM with an implicit sync cycle and
    // the starter continues; the TCM's first statement runs after the
    // starter's turn.
    let output = clean_output(
        "extend sys {\n    tcm() @sys.any is {\n        out(\"TCM\");\n    };\n    run() is also {\n        \
         start tcm();\n        out(\"MAIN\");\n    };\n};\n",
        1,
    );
    let main_pos = output.find("MAIN").expect("MAIN printed");
    let tcm_pos = output.find("TCM").expect("TCM printed");
    assert!(main_pos < tcm_pos, "the starter continues first: {output}");
}

// --- Clause 33 resource-sharing structs ---

#[test]
fn semaphore_values_queries_and_transfers_run() {
    // 31.2.3/31.2.4: a fresh semaphore starts at 0 available under a
    // MAX_INT maximum; set_value/set_max_value configure it before any
    // up()/down() was called; up()/down() transfer one resource each.
    let output = clean_output(
        "extend sys {\n    run() is also {\n        var a: semaphore = new semaphore;\n        \
         out(\"value=\", a.get_value(), \" max=\", a.get_max_value(), \"\\n\");\n        \
         a.set_value(2);\n        out(\"set=\", a.get_value(), \"\\n\");\n        \
         var b: semaphore = new semaphore;\n        b.set_max_value(3);\n        \
         out(\"max=\", b.get_max_value(), \"\\n\");\n        \
         b.up();\n        b.up();\n        out(\"ups=\", b.get_value(), \"\\n\");\n        \
         b.down();\n        out(\"down=\", b.get_value(), \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(
        output,
        "value=0 max=2147483647\nset=2\nmax=3\nups=2\ndown=1\n"
    );
}

#[test]
fn try_up_and_try_down_never_block_and_report_the_change() {
    // 31.2.2: try_down() on an empty semaphore is FALSE without
    // blocking; after an up() it is TRUE and the value moved.
    let output = clean_output(
        "extend sys {\n    run() is also {\n        var s: semaphore = new semaphore;\n        \
         if not s.try_down() { out(\"EMPTY\\n\"); };\n        s.up();\n        \
         if s.try_down() { out(\"TOOK\\n\"); };\n        out(\"value=\", s.get_value(), \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "EMPTY\nTOOK\nvalue=0\n");
}

#[test]
fn down_blocks_until_a_producer_ups() {
    // 31.2.1: the consumer blocks at down() (no resource available);
    // the producer's up() two cycles later makes it proceed. The
    // blocking calls yield at the call (a TCM call is an interrupt
    // point), so the producer announces the up() before making it —
    // the consumer can only resume once the resource exists.
    let output = clean_output(
        "extend sys {\n    s: semaphore;\n    consumer() @sys.any is {\n        \
         out(\"CONSUMER WAIT\\n\");\n        s.down();\n        out(\"CONSUMER GOT\\n\");\n    };\n    \
         producer() @sys.any is {\n        wait [2]*cycle;\n        out(\"PRODUCER UP\\n\");\n        \
         s.up();\n    };\n    run() is also {\n        s = new semaphore;\n        \
         start consumer();\n        start producer();\n    };\n};\n",
        1,
    );
    let wait_pos = output.find("CONSUMER WAIT").expect("consumer parked");
    let up_pos = output.find("PRODUCER UP").expect("producer produced");
    let got_pos = output.find("CONSUMER GOT").expect("consumer resumed");
    assert!(wait_pos < up_pos, "the consumer blocks first: {output}");
    assert!(
        up_pos < got_pos,
        "the consumer resumes after the up(): {output}"
    );
}

#[test]
fn blocked_consumers_are_serviced_fifo() {
    // 31.2.1: blocked down() calls are serviced in their request order.
    // Two consumers block; one up() services the first only; the second
    // up() services the second.
    let output = clean_output(
        "extend sys {\n    s: semaphore;\n    c1() @sys.any is {\n        s.down();\n        \
         out(\"C1\\n\");\n    };\n    c2() @sys.any is {\n        s.down();\n        \
         out(\"C2\\n\");\n    };\n    producer() @sys.any is {\n        s.up();\n        \
         wait cycle;\n        s.up();\n        out(\"PRODUCED\\n\");\n    };\n    \
         run() is also {\n        s = new semaphore;\n        start c1();\n        start c2();\n        \
         start producer();\n    };\n};\n",
        1,
    );
    let c1 = output.find("C1").expect("first consumer served");
    let c2 = output.find("C2").expect("second consumer served");
    assert!(
        c1 < c2,
        "the first blocked consumer is serviced first: {output}"
    );
    assert!(
        output.find("PRODUCED").is_some(),
        "the producer completes: {output}"
    );
}

#[test]
fn try_never_beats_a_pending_blocked_call() {
    // 31.2.2: "a try_up() shall never succeed if there are pending
    // calls to up()" — symmetric for down. A blocked down() is pending;
    // a try_down() must fail even though an up() just added a resource
    // (the blocked call is serviced first).
    let output = clean_output(
        "extend sys {\n    s: semaphore;\n    consumer() @sys.any is {\n        s.down();\n        \
         out(\"GOT\\n\");\n    };\n    prober() @sys.any is {\n        wait [2]*cycle;\n        \
         s.up();\n        if s.try_down() { out(\"TRIED\\n\"); } else { out(\"DENIED\\n\"); };\n    };\n    \
         run() is also {\n        s = new semaphore;\n        start consumer();\n        \
         start prober();\n    };\n};\n",
        1,
    );
    // The blocked consumer is serviced first; the prober's try_down()
    // runs while no down() is pending anymore... — the resource went to
    // the blocked call, so the try finds an empty semaphore: DENIED.
    assert!(
        output.contains("DENIED"),
        "the blocked call wins the resource over the try: {output}"
    );
    assert!(
        output.contains("GOT"),
        "the blocked consumer completes: {output}"
    );
}

#[test]
fn set_value_after_a_transfer_is_e0422() {
    let result = run_source(
        "extend sys {\n    run() is also {\n        var s: semaphore = new semaphore;\n        \
         s.up();\n        s.set_value(1);\n    };\n};\n",
        1,
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == "E0422" && d.message.contains("set_value")),
        "diagnostics: {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
}

#[test]
fn set_max_below_the_current_value_is_e0422() {
    let result = run_source(
        "extend sys {\n    run() is also {\n        var s: semaphore = new semaphore;\n        \
         s.set_value(3);\n        s.set_max_value(2);\n    };\n};\n",
        1,
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == "E0422" && d.message.contains("set_max_value")),
        "diagnostics: {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_blocking_call_outside_a_tcm_context_is_e0421() {
    // down() on an empty semaphore inside run()'s main coroutine IS a
    // coroutine — the block is legal there. Inside an inline (atomic)
    // method it cannot suspend: E0421.
    let result = run_source(
        "struct helper_s {\n    s: semaphore;\n    take() is {\n        s.down();\n    };\n};\n\
         extend sys {\n    run() is also {\n        var h: helper_s = new helper_s;\n        \
         h.s = new semaphore;\n        h.take();\n    };\n};\n",
        1,
    );
    assert!(
        result.diagnostics.iter().any(|d| d.code == "E0421"),
        "diagnostics: {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
}

#[test]
fn locker_mutual_exclusion_and_fifo_grant() {
    // 31.2.5: the first lock() holds; the second blocks; release()
    // hands the lock to the waiting TCM, in request order. The blocking
    // calls yield at the call, so each TCM announces the action before
    // making it — the waiter's acquire print proves the grant happened
    // after the holder announced its release.
    let output = clean_output(
        "extend sys {\n    l: locker;\n    t1() @sys.any is {\n        l.lock();\n        \
         out(\"F-LOCKED\\n\");\n        wait [2]*cycle;\n        out(\"F-RELEASING\\n\");\n        \
         l.release();\n    };\n    t2() @sys.any is {\n        wait cycle;\n        \
         l.lock();\n        out(\"S-LOCKED\\n\");\n        l.release();\n        \
         out(\"S-DONE\\n\");\n    };\n    run() is also {\n        l = new locker;\n        \
         start t1();\n        start t2();\n    };\n};\n",
        1,
    );
    let f_locked = output.find("F-LOCKED").expect("first acquires");
    let f_releasing = output.find("F-RELEASING").expect("first announces release");
    let s_locked = output
        .find("S-LOCKED")
        .expect("second acquires after release");
    let s_done = output.find("S-DONE").expect("second finishes");
    assert!(f_locked < s_locked, "the first lock wins: {output}");
    assert!(
        f_releasing < s_locked,
        "the second acquires only after the release is announced: {output}"
    );
    assert!(s_locked < s_done, "the second releases: {output}");
}

#[test]
fn release_by_an_entity_that_does_not_hold_is_e0423() {
    let result = run_source(
        "extend sys {\n    l: locker;\n    run() is also {\n        l = new locker;\n        \
         l.release();\n    };\n};\n",
        1,
    );
    assert!(
        result.diagnostics.iter().any(|d| d.code == "E0423"),
        "diagnostics: {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
}

#[test]
fn rdv_semaphore_pairs_up_and_down_up_first() {
    // 31.2.1: an rdv_semaphore has no resources; up() and down() block
    // until they coincide, and the paired up() thread always runs
    // first, followed immediately by the down() thread.
    let output = clean_output(
        "extend sys {\n    r: rdv_semaphore;\n    producer() @sys.any is {\n        \
         out(\"P-BLOCK\\n\");\n        r.up();\n        out(\"P-MET\\n\");\n    };\n    \
         consumer() @sys.any is {\n        wait [2]*cycle;\n        r.down();\n        \
         out(\"C-MET\\n\");\n    };\n    run() is also {\n        r = new rdv_semaphore;\n        \
         start producer();\n        start consumer();\n    };\n};\n",
        1,
    );
    let p_block = output.find("P-BLOCK").expect("producer parked");
    let p_met = output.find("P-MET").expect("producer met");
    let c_met = output.find("C-MET").expect("consumer met");
    assert!(p_block < p_met, "the producer blocks first: {output}");
    assert!(p_met < c_met, "the paired up() thread runs first: {output}");
}

#[test]
fn rdv_semaphore_value_stays_zero() {
    let result = run_source(
        "extend sys {\n    run() is also {\n        var r: rdv_semaphore = new rdv_semaphore;\n        \
         r.set_value(1);\n    };\n};\n",
        1,
    );
    assert!(
        result.diagnostics.iter().any(|d| d.code == "E0422"),
        "diagnostics: {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
}

#[test]
fn invariant_expect_rule_fires_live_before_the_run_ends() {
    // 11.4, live monitoring: `not @bad_ev` is an invariant — the
    // occurrence of the forbidden behavior exhausts every possibility
    // at that cycle, so the rule fires at the next cycle boundary, not
    // at quit. The late TCM (still sleeping when the failure becomes
    // certain) prints strictly after the rule's else body ran.
    let output = clean_output(
        "extend sys {\n    bad_ev: event;\n    \
         expect not @bad_ev else { out(\"FIRED\\n\"); };\n    \
         emitter() @sys.any is {\n        wait [3]*cycle;\n        emit bad_ev;\n        \
         out(\"AFTER\\n\");\n    };\n    late() @sys.any is {\n        wait [6]*cycle;\n        \
         out(\"LATE\\n\");\n    };\n    run() is also {\n        start emitter();\n        \
         start late();\n    };\n};\n",
        1,
    );
    let fired = output.find("FIRED").expect("the rule fired");
    let after = output.find("AFTER").expect("emitter continued");
    let late = output.find("LATE").expect("late TCM ran");
    assert!(
        after < fired || fired < late,
        "the failure fired mid-run: {output}"
    );
    assert!(
        fired < late,
        "the failure is reported before the later cycle's print: {output}"
    );
}

#[test]
fn temporal_wait_in_a_tcm_observes_emissions_made_while_suspended() {
    // The marquee clause 13.2.2 behavior: the waiter suspends at
    // `wait @ev`; the emitter's TCM advances two cycles and emits; the
    // waiter resumes and completes.
    let output = clean_output(
        "extend sys {\n    ev: event;\n    waiter() @sys.any is {\n        wait @ev;\n        \
         out(\"A RESUMED\");\n    };\n    emitter() @sys.any is {\n        wait [2]*cycle;\n        \
         emit ev;\n        out(\"B EMITTED\");\n    };\n    run() is also {\n        \
         start waiter();\n        start emitter();\n    };\n};\n",
        1,
    );
    assert!(output.contains("B EMITTED"), "output: {output}");
    assert!(output.contains("A RESUMED"), "output: {output}");
    let b_pos = output.find("B EMITTED").expect("B");
    let a_pos = output.find("A RESUMED").expect("A");
    assert!(b_pos < a_pos, "the waiter resumes after the emit: {output}");
}

#[test]
fn run_waits_suspend_and_started_tcms_progress() {
    // The main coroutine suspends at `wait @ev`; the started TCM emits
    // one cycle later; the main coroutine resumes.
    let output = clean_output(
        "extend sys {\n    ev: event;\n    tcm() @sys.any is {\n        wait [1]*cycle;\n        \
         emit ev;\n    };\n    run() is also {\n        start tcm();\n        wait @ev;\n        \
         out(\"RUN RESUMED\");\n    };\n};\n",
        1,
    );
    assert!(output.contains("RUN RESUMED"), "output: {output}");
}

#[test]
fn tcm_locals_persist_across_suspensions_in_a_while_loop() {
    // The resume cursor re-enters the while body mid-iteration and the
    // captured frame keeps the locals: the loop counts to 3 across
    // three suspensions.
    let output = clean_output(
        "extend sys {\n    tcm() @sys.any is {\n        var i: uint = 0;\n        while i < 3 {\n            \
         i = i + 1;\n            wait [1]*cycle;\n        };\n        out(\"COUNTED\");\n        \
         print i;\n    };\n    run() is also {\n        start tcm();\n    };\n};\n",
        1,
    );
    assert!(output.contains("COUNTED"), "output: {output}");
    assert!(output.contains("i = 3"), "output: {output}");
}

#[test]
fn bare_sync_suspends_for_one_cycle() {
    // 13.2.3: `sync;` suspends for one occurrence of the default
    // sampling event. The TCM syncs away cycle 0 and its wait starts
    // at cycle 1; the starter's emission lands in cycle 2 and the
    // post-sync wait observes it from the log.
    let output = clean_output(
        "extend sys {\n    ev: event;\n    seen: uint;\n    tcm() @sys.any is {\n        \
         sync;\n        wait @ev;\n        out(\"SAW EV\");\n    };\n    run() is also {\n        \
         start tcm();\n        wait [2]*cycle;\n        emit ev;\n    };\n};\n",
        1,
    );
    assert!(output.contains("SAW EV"), "output: {output}");
}

#[test]
fn wait_inside_an_if_body_suspends_and_resumes() {
    // The marquee generalization: the wait inside the `then` branch
    // suspends the TCM (13.2.2) while the emitter advances; the resume
    // path re-enters the branch without re-evaluating the condition or
    // replaying the branch's executed statements.
    let output = clean_output(
        "extend sys {\n    ev: event;\n    waiter() @sys.any is {\n        var go: bool = TRUE;\n        \
         if go {\n            wait @ev;\n            out(\"A RESUMED\");\n        };\n        \
         out(\"A DONE\");\n    };\n    emitter() @sys.any is {\n        wait [2]*cycle;\n        \
         emit ev;\n        out(\"B EMITTED\");\n    };\n    run() is also {\n        \
         start waiter();\n        start emitter();\n    };\n};\n",
        1,
    );
    assert!(output.contains("B EMITTED"), "output: {output}");
    assert!(output.contains("A RESUMED"), "output: {output}");
    assert!(output.contains("A DONE"), "output: {output}");
    let b_pos = output.find("B EMITTED").expect("B");
    let a_pos = output.find("A RESUMED").expect("A");
    assert!(b_pos < a_pos, "the waiter resumes after the emit: {output}");
}

#[test]
fn wait_inside_an_if_else_body_suspends_and_resumes() {
    // The resume path records the else branch too: the TCM re-enters
    // it directly, without re-evaluating the condition.
    let output = clean_output(
        "extend sys {\n    ev: event;\n    tcm() @sys.any is {\n        var go: bool = FALSE;\n        \
         if go {\n            out(\"THEN\\n\");\n        } else {\n            wait @ev;\n            \
         out(\"ELSE RESUMED\\n\");\n        };\n    };\n    emitter() @sys.any is {\n        \
         wait [2]*cycle;\n        emit ev;\n    };\n    run() is also {\n        start tcm();\n        \
         start emitter();\n    };\n};\n",
        1,
    );
    assert!(output.contains("ELSE RESUMED"), "output: {output}");
    assert!(!output.contains("THEN"), "output: {output}");
}

#[test]
fn wait_inside_a_for_body_keeps_the_counter() {
    // The for loop's temporary is bound in the frame and the
    // in-flight counter rides the resume path: three suspensions (one
    // per iteration) resume mid-iteration without resetting the
    // counter or re-evaluating the bounds.
    let output = clean_output(
        "extend sys {\n    tcm() @sys.any is {\n        for i from 1 to 3 {\n            \
         wait [1]*cycle;\n            outf(\"%d\\n\", i);\n        };\n    };\n    run() is also {\n        \
         start tcm();\n    };\n};\n",
        1,
    );
    assert_eq!(output, "1\n2\n3\n");
}

#[test]
fn wait_inside_a_case_arm_suspends_and_resumes() {
    // The matched arm is recorded on the resume path: the TCM
    // re-enters it without re-matching the scrutinee.
    let output = clean_output(
        "extend sys {\n    ev: event;\n    tcm() @sys.any is {\n        var x: uint = 2;\n        \
         case x {\n            1: { out(\"ONE\\n\"); };\n            [2, 7]: {\n                \
         wait @ev;\n                out(\"TWO-OR-SEVEN RESUMED\\n\");\n            };\n            \
         default: { out(\"OTHER\\n\"); };\n        };\n    };\n    emitter() @sys.any is {\n        \
         wait [2]*cycle;\n        emit ev;\n    };\n    run() is also {\n        start tcm();\n        \
         start emitter();\n    };\n};\n",
        1,
    );
    assert!(output.contains("TWO-OR-SEVEN RESUMED"), "output: {output}");
    assert!(!output.contains("OTHER"), "output: {output}");
}

#[test]
fn wait_inside_a_repeat_until_body_suspends_and_resumes() {
    // A repeat-until body re-enters mid-iteration (the condition is
    // only evaluated when the iteration completes, 20.2.3): the first
    // wait resumes on the emission and the second succeeds in the same
    // cycle's log (the occurrence is behind the fresh wait's start).
    let output = clean_output(
        "extend sys {\n    ev: event;\n    tcm() @sys.any is {\n        var n: uint = 0;\n        \
         repeat {\n            n = n + 1;\n            wait @ev;\n            outf(\"%d\\n\", n);\n        \
         } until n == 2;\n    };\n    emitter() @sys.any is {\n        for j from 1 to 2 {\n            \
         wait [2]*cycle;\n            emit ev;\n        };\n    };\n    run() is also {\n        \
         start tcm();\n        start emitter();\n    };\n};\n",
        1,
    );
    // The body counts exactly two waits (the second succeeds on the
    // same cycle's log); a third line would mean the loop ran past its
    // condition.
    assert!(output.contains("1\n2\n"), "output: {output}");
    assert!(!output.contains("3"), "output: {output}");
}

#[test]
fn wait_inside_a_for_each_body_suspends_and_resumes() {
    // The list snapshot rides the resume path: the element and index
    // bindings persist in the frame across the suspension.
    let output = clean_output(
        "extend sys {\n    tcm() @sys.any is {\n        var l: list of uint;\n        l.push(1);\n        \
         l.push(2);\n        for each (v) using index (idx) in l {\n            wait [1]*cycle;\n            \
         print v, idx;\n        };\n    };\n    run() is also {\n        start tcm();\n    };\n};\n",
        1,
    );
    assert_eq!(output, "v = 1 idx = 0\nv = 2 idx = 1\n");
}

#[test]
fn wait_inside_an_if_inside_a_while_suspends_and_resumes() {
    // A multi-level path: the suspension inside while > if > wait
    // re-enters through both compounds, the branch's statement after
    // the wait executes first (skip-resume), and the loop's
    // mid-iteration semantics survive the added nesting.
    let output = clean_output(
        "extend sys {\n    tcm() @sys.any is {\n        var i: uint = 0;\n        while i < 2 {\n            \
         i = i + 1;\n            if TRUE {\n                wait [1]*cycle;\n                \
         out(\"WAITED\\n\");\n            };\n            out(\"ITER\\n\");\n        };\n        print i;\n    };\n    \
         run() is also {\n        start tcm();\n    };\n};\n",
        1,
    );
    assert_eq!(output, "WAITED\nITER\nWAITED\nITER\ni = 2\n");
}

#[test]
fn wait_at_the_end_of_an_if_body_resumes_past_the_branch() {
    // The sync consumed by a branch's last statement resumes past the
    // body's end (the skip position falls out of the branch), and the
    // statement following the compound runs.
    let output = clean_output(
        "extend sys {\n    tcm() @sys.any is {\n        if TRUE {\n            wait [1]*cycle;\n        };\n        \
         out(\"AFTER\\n\");\n    };\n    run() is also {\n        start tcm();\n    };\n};\n",
        1,
    );
    assert_eq!(output, "AFTER\n");
}

#[test]
fn runaway_tcm_hits_the_wait_budget() {
    let result = run_source(
        "extend sys {\n    run() is also {\n        while TRUE {\n            wait cycle;\n        };\n    };\n};\n",
        1,
    );
    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec!["E0409"]
    );
}

#[test]
fn implication_and_not_evaluate_in_constraints() {
    // `a => b` with a held TRUE forces b to gen TRUE; `not b` excludes
    // FALSE from the second field.
    let output = clean_output(
        "struct s {\n    a: bool;\n    b: bool;\n    keep a == TRUE => b == TRUE;\n    \
         keep not (b == FALSE);\n};\n\
         extend sys {\n    run() is {\n        var x: s;\n        gen x;\n        print x.a, x.b;\n    };\n};\n",
        7,
    );
    assert!(output.contains("TRUE"), "output: {output}");
}

#[test]
fn bind_constraints_are_skipped_by_gen() {
    // A bind constraint names HDL-side signals; gen must not treat it as
    // a rejection predicate over e fields.
    let output = clean_output(
        "struct s {\n    v: uint(bits: 8);\n    keep bind (sig_clk, external);\n    \
         keep v < 10;\n};\n\
         extend sys {\n    run() is {\n        var x: s;\n        gen x;\n        print x.v;\n    };\n};\n",
        3,
    );
    assert!(output.contains('\n'), "output: {output}");
}

#[test]
fn is_a_checks_scalar_named_and_like_types() {
    let output = clean_output(
        "struct base_s {\n    f: uint;\n};\n\
         struct child_s like base_s {\n    g: uint;\n};\n\
         extend sys {\n    run() is also {\n        var c: child_s;\n        \
         print c is a child_s;\n        print c is a base_s;\n        print c is not a base_s;\n        \
         var i: int = 3;\n        print i is a int;\n        print i is a real;\n        \
         print i is a string;\n        print \"text\" is a string;\n        \
         var l: list of uint;\n        print l is a list of uint;\n    };\n};\n",
        1,
    );
    assert_eq!(
        output,
        "TRUE\nTRUE\nFALSE\nTRUE\nFALSE\nFALSE\nTRUE\nTRUE\n"
    );
}

#[test]
fn is_a_checks_enumeral_types_by_name() {
    let output = clean_output(
        "type color_t: [red, green];\n\
         extend sys {\n    run() is also {\n        var c: color_t = red;\n        \
         print c is a color_t;\n        print c is not a color_t;\n    };\n};\n",
        1,
    );
    assert_eq!(output, "TRUE\nFALSE\n");
}

#[test]
fn string_match_evaluates_case_insensitive_full_string_literals() {
    // Native string matching (clause 4.12.1) is case insensitive and
    // always matches the full string; the literal subset compares
    // equal-ignoring-case.
    let output = clean_output(
        "extend sys {\n    run() is also {\n        \
         print \"hello\" ~ \"hello\";\n        print \"hello\" ~ \"HELLO\";\n        \
         print \"hello\" !~ \"world\";\n        print \"hello\" !~ \"hell\";\n    };\n};\n",
        1,
    );
    assert_eq!(output, "TRUE\nTRUE\nTRUE\nTRUE\n");
}

#[test]
fn string_match_reports_unsupported_pattern_shapes() {
    // `/.../` selects AWK-style regular expressions (clause 4.12.2) and
    // native meta-characters are unimplemented; both report E0410.
    let result = run(
        &elaborate(
            &parse(
                "extend sys {\n    run() is also {\n        print \"x\" ~ \"/ab+/\";\n    };\n};\n",
            )
            .declarations,
        ),
        1,
    );
    assert_eq!(result.diagnostics.len(), 1);
    assert!(
        result.diagnostics[0].render().contains("E0410"),
        "expected E0410: {:?}",
        result.diagnostics[0].render()
    );
    let result = run(
        &elaborate(
            &parse(
                "extend sys {\n    run() is also {\n        print \"x\" ~ \"a*b\";\n    };\n};\n",
            )
            .declarations,
        ),
        1,
    );
    assert_eq!(result.diagnostics.len(), 1);
    assert!(
        result.diagnostics[0].render().contains("E0410"),
        "expected E0410: {:?}",
        result.diagnostics[0].render()
    );
}

#[test]
fn emit_runs_on_blocks_in_declaration_order() {
    // `emit` executes the event's `on` action blocks immediately, in
    // composition order (clause 11.2), including through a dotted emit
    // that resolves the owning instance and repeated `on` declarations.
    let output = clean_output(
        "unit env {\n    event reset_done;\n};\n\
         extend sys {\n    env_i : env is instance;\n};\n\
         extend env {\n    on reset_done {\n        out(\"one\\n\");\n    };\n    \
         on reset_done {\n        out(\"two\\n\");\n    };\n};\n\
         extend sys {\n    run() is also {\n        emit sys.env_i.reset_done;\n        \
         out(\"after\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "one\ntwo\nafter\n");
}

#[test]
fn emit_of_event_without_on_blocks_is_silent() {
    let output = clean_output(
        "unit env {\n    event reset_done;\n};\n\
         extend sys {\n    env_i : env is instance;\n    run() is also {\n        \
         emit sys.env_i.reset_done;\n        out(\"done\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "done\n");
}

#[test]
fn gen_respects_in_range_constraints_on_unsized_int() {
    // Without domain narrowing, an unsized `int` samples the full 32-bit
    // domain and rejection sampling never hits `in [1..100]` (E0407);
    // the sampling domain narrows to the constraint range.
    let output = clean_output(
        "struct packet {\n    tail: int;\n};\n\
         extend packet {\n    keep tail in [1..100];\n};\n\
         extend sys {\n    run() is also {\n        var p: packet;\n        gen p;\n        \
         if p.tail < 1 or p.tail > 100 {\n            out(\"BAD\\n\");\n        } else {\n            \
         out(\"OK\\n\");\n        };\n    };\n};\n",
        1,
    );
    assert_eq!(output, "OK\n");
}

#[test]
fn gen_respects_equality_constraints() {
    let output = clean_output(
        "struct s {\n    v: int;\n    w: uint(bits: 8);\n};\n\
         extend s {\n    keep v == 7;\n    keep 42 == w;\n};\n\
         extend sys {\n    run() is also {\n        var x: s;\n        gen x;\n        \
         print x.v, x.w;\n    };\n};\n",
        1,
    );
    assert_eq!(output, "7 42\n");
}

#[test]
fn gen_intersects_overlapping_range_constraints() {
    // Two conjunctive ranges narrow to their intersection: [5..10].
    let output = clean_output(
        "struct s {\n    v: int;\n};\n\
         extend s {\n    keep v in [1..10];\n    keep v in [5..20];\n};\n\
         extend sys {\n    run() is also {\n        var x: s;\n        gen x;\n        \
         if x.v < 5 or x.v > 10 {\n            out(\"BAD\\n\");\n        } else {\n            \
         out(\"OK\\n\");\n        };\n    };\n};\n",
        1,
    );
    assert_eq!(output, "OK\n");
}

#[test]
fn gen_domain_narrowing_leaves_unconstrained_fields_alone() {
    // A constrained field must not stop other fields from sampling
    // structurally (byte stays uniform over 0..255).
    let output = clean_output(
        "struct s {\n    t: int;\n    b: byte;\n};\n\
         extend s {\n    keep t in [1..100];\n};\n\
         extend sys {\n    run() is also {\n        var x: s;\n        gen x;\n        \
         if x.t < 1 or x.t > 100 or x.b < 0 or x.b > 255 {\n            out(\"BAD\\n\");\n        \
         } else {\n            out(\"OK\\n\");\n        };\n    };\n};\n",
        1,
    );
    assert_eq!(output, "OK\n");
}

#[test]
fn longint_and_longuint_hold_64_bit_values() {
    // The long bases (Table 17, clause 5.2.3.1) are signed/unsigned
    // 64-bit: initialization and out() carry the full values, including
    // the unsigned maximum 2^64-1, untruncated.
    let output = clean_output(
        "<'\nextend sys {\n    run() is also {\n        var a: longuint = 1;\n        \
         var b: longint = -1;\n        out(a, \"\\n\");\n        out(b, \"\\n\");\n        \
         var c: longuint = 18446744073709551615;\n        out(c, \"\\n\");\n    };\n};\n'>\n",
        3,
    );
    assert_eq!(output, "1\n-1\n18446744073709551615\n");
}

#[test]
fn gen_of_long_bases_samples_the_full_64_bit_range() {
    // An unconstrained `longuint` samples the full unsigned 64-bit range
    // and a `longint` the full signed one: constraints in the top quarter
    // of the unsigned range and the bottom quarter of the signed range are
    // both satisfiable. A sampler capped at 32 bits would exhaust the
    // 1000 rejection attempts and report E0407.
    let output = clean_output(
        "struct s {\n    u: longuint;\n    i: longint;\n    \
         keep u > 9223372036854775808;\n    keep i < -4611686018427387904;\n};\n\
         extend sys {\n    run() is also {\n        var p: s;\n        gen p;\n        \
         var x: longint;\n        gen x;\n        out(\"OK\\n\");\n    };\n};\n",
        5,
    );
    assert_eq!(output, "OK\n");
}

#[test]
fn down_to_counts_and_reverse_for_each_binds_the_index() {
    let output = clean_output(
        "extend sys {\n    run() is also {\n        var l: list of int;\n        \
         l.push(10); l.push(20); l.push(30);\n        \
         for i from 3 down to 1 {\n            out(i, \"\\n\");\n        };\n        \
         for each (v) using index (idx) in reverse l {\n            \
         out(idx, \"=\", v, \"\\n\");\n        };\n    };\n};\n",
        1,
    );
    assert_eq!(output, "3\n2\n1\n0=30\n1=20\n2=10\n");
}

#[test]
fn is_a_binding_exposes_the_matched_value() {
    let output = clean_output(
        "struct packet_s {\n    x: int;\n};\n\
         extend sys {\n    run() is also {\n        var p: packet_s;\n        \
         if p is a packet_s (cp) {\n            out(\"x=\", cp.x, \"\\n\");\n        } else {\n            \
         out(\"no match\\n\");\n        };\n    };\n};\n",
        1,
    );
    assert_eq!(output, "x=0\n");
}

#[test]
fn compute_calls_inline_and_untyped_params_accept_anything() {
    let output = clean_output(
        "struct s {\n    bump(v) is {\n        return v + 1;\n    };\n    \
         note() is {\n            out(\"computed\\n\");\n        };\n    \
         twice(): uint is {\n        compute note();\n        return 2;\n    };\n};\n\
         extend sys {\n    run() is also {\n        var m: s;\n        \
         out(m.bump(4), \"\\n\");\n        print m.twice();\n    };\n};\n",
        1,
    );
    assert_eq!(output, "5\ncomputed\n2\n");
}

#[test]
fn new_reports_a_clean_runtime_diagnostic() {
    // An untyped `new` without a declared type to infer from (here: an
    // assignment target's field has no type known to `new`) stays
    // unsupported.
    let result = run(
        &elaborate(
            &parse(
                "struct s {\n    v: int;\n};\n\
             extend sys {\n    p: s;\n    run() is also {\n        p = new;\n    };\n};\n",
            )
            .declarations,
        ),
        1,
    );
    assert_eq!(result.diagnostics.len(), 1);
    assert!(
        result.diagnostics[0].render().contains("E0402"),
        "expected E0402 for 'new': {:?}",
        result.diagnostics[0].render()
    );
}

#[test]
fn untyped_new_creates_the_var_declared_type() {
    // `var p: s = new;` — the declared type is the instance (clause 18).
    let output = clean_output(
        "struct s {\n    v: int;\n};\n\
         extend sys {\n    run() is also {\n        var p: s = new;\n        p.v = 7;\n        out(p.v, \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "7\n");
}

#[test]
fn untyped_new_in_a_typed_var_declaration_takes_the_var_type() {
    // `var c: cfg = new;` — clause 6.3's typed creation where the type
    // comes from the assignment target.
    let output = clean_output(
        "struct s {\n    v: int;\n};\n\
         extend sys {\n    run() is also {\n        var c: s = new;\n        out(c.v, \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "0\n");
}

#[test]
fn field_init_values_are_set_at_instance_creation() {
    // Clause 6.9.1/6.9.2/6.9.4: `!` and `const !` fields with init
    // values; the values are set on instance creation, before `init()`
    // would run. (The plan's list-literal init `{1; 0; 1}` needs the
    // list-concat expression, which is not on this base — the third
    // field is a scalar instead; see the T1.11 deviation note.)
    let output = clean_output(
        "<'\nstruct cfg {\n    !depth: uint = 16;\n    !name: string = \"dut\";\n    !radix: uint = 8;\n    const !limit: uint = 3;\n};\n\
         extend sys {\n    run() is also {\n        var c: cfg = new;\n        out(c.depth, \"\\n\");\n        out(c.name, \"\\n\");\n        out(c.radix, \"\\n\");\n        out(c.limit, \"\\n\");\n    };\n};\n'>\n",
        1,
    );
    assert_eq!(output, "16\ndut\n8\n3\n");
}

#[test]
fn field_init_values_apply_to_every_instance() {
    // Clause 6.9.4: each instance creation re-applies the init value,
    // and `gen` does not overwrite it on a `!` field.
    let output = clean_output(
        "struct cfg {\n    !seed: uint = 7;\n};\n\
         extend sys {\n    run() is also {\n        var a: cfg = new;\n        var b: cfg = new;\n        out(a.seed, \" \", b.seed, \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "7 7\n");
}

#[test]
fn const_field_keeps_its_init_value_through_gen() {
    // Clause 6.9.4: a const field is initialised once — by its init
    // value here — so `gen` must not overwrite it.
    let output = clean_output(
        "struct cfg {\n    const limit: uint = 3;\n};\n\
         extend sys {\n    run() is also {\n        var c: cfg = new;\n        gen c;\n        out(c.limit, \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "3\n");
}

#[test]
fn static_prefix_is_a_contextual_word() {
    // Clause 6.9.3: `static` prefixes a member only when a member
    // declarator follows; a member genuinely named `static` keeps
    // parsing as before.
    let result = parse("struct s {\n    static count: uint;\n};\n");
    assert!(
        result.diagnostics.is_empty(),
        "a static-prefixed field parses (T2.1): {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
    let named = parse("struct s {\n    static: uint;\n    m: uint;\n};\n");
    assert!(
        named.diagnostics.is_empty(),
        "a field named 'static' must keep parsing: {:?}",
        named
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
}

#[test]
fn static_field_is_shared_per_type_and_created_at_declaration() {
    // T2.1 (clause 6.9.3): one shared occurrence per type — zero
    // instances still read the declaration-time value, every `init()`
    // bumps the same counter, and the `!`-prefixed static string keeps
    // its declaration value. Static fields are excluded from instance
    // formatting (`print`/`to_string`).
    let output = clean_output(
        "<'\nstruct counter {\n    static count: uint;\n    static !label: string = \"ctr\";\n    id: uint;\n    init() is also { counter::count = counter::count + 1; };\n};\nextend sys {\n    run() is also {\n        out(counter::count, \"\\n\");\n        var a: counter = new; var b: counter = new; var c: counter = new;\n        out(counter::count, \"\\n\");\n        out(counter::label, \"\\n\");\n        out(a.id == b.id or TRUE, \"\\n\");\n        print a;\n    };\n};\n'>\n",
        1,
    );
    assert_eq!(output, "0\n3\nctr\nTRUE\na = counter{id: 0}\n");
}

#[test]
fn static_method_dispatches_without_an_instance() {
    // T2.2 (clause 17.2.2): `Type::name(...)` calls without an
    // instance; a static method may call a sibling static method
    // through the type.
    let output = clean_output(
        "<'\nstruct math {\n    static sum(a: int, b: int): int is { return a + b; };\n    static twice(a: int): int is { return math::sum(a, a); };\n};\nextend sys {\n    run() is also { out(math::sum(2, 3), \"\\n\"); out(math::twice(21), \"\\n\"); };\n};\n'>\n",
        1,
    );
    assert_eq!(output, "5\n42\n");
}

#[test]
fn static_method_rejects_instance_dispatch() {
    // T2.2 (clause 17.2.2): the instance call form has no static
    // dispatch — a static method has no `me`, so the dot operator does
    // not reach it.
    let result = run_source(
        "<'\nstruct sm {\n    static f(): uint is { return 3; };\n};\nextend sys {\n    run() is also {\n        var n: sm = new;\n        out(n.f());\n    };\n};\n'>\n",
        1,
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.render().contains("E0402") && d.render().contains("static method")),
        "expected the static-dispatch diagnostic: {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
}

#[test]
fn static_event_fires_both_on_forms_with_no_instance() {
    // T2.3 (clauses 11.1.4.1, 11.2): `emit Type::event` triggers the
    // declaring type's `static on` blocks and every type-qualified
    // `on Type::event` block, with no instance.
    let output = clean_output(
        "<'\nstruct bus {\n    static event reset;\n    static on reset { out(\"bus reset seen\\n\"); };\n};\nextend bus {\n    on bus::reset { out(\"qualified reset seen\\n\"); };\n};\nextend sys {\n    run() is also { emit bus::reset; emit bus::reset; };\n};\n'>\n",
        1,
    );
    assert_eq!(
        output,
        "bus reset seen\nqualified reset seen\nbus reset seen\nqualified reset seen\n"
    );
}

#[test]
fn static_field_bare_name_resolves_inside_the_type() {
    // T2.1 (clause 6.9.3): inside the type, the static field is
    // reachable by its bare name — read and write — and a path may
    // start at `Type::field` and continue through non-static fields.
    let output = clean_output(
        "<'\nstruct inner {\n    v: uint;\n};\nstruct holder {\n    static box: inner;\n    static tally: uint;\n    bump() is { tally = tally + 1; };\n    get_tally(): uint is { return tally; };\n};\nextend sys {\n    run() is also {\n        var n: holder = new;\n        n.bump();\n        out(n.get_tally(), \"\\n\");\n        out(holder::box.v == 0, \"\\n\");\n    };\n};\n'>\n",
        1,
    );
    assert_eq!(output, "1\nTRUE\n");
}

#[test]
fn select_constraints_are_no_ops_for_the_sampler() {
    // A `keep soft x == select { .. }` distribution parses and is
    // treated as a no-op by the rejection sampler (clause 10's
    // distribution biasing is a later arc); gen still produces values.
    let output = clean_output(
        "struct s {\n    v: uint (bits: 8);\n};\n\
         extend s {\n    keep soft v == select {\n        50 : min;\n        25 : max;\n        \
         25 : others;\n    };\n};\n\
         extend sys {\n    run() is also {\n        var x: s;\n        gen x;\n        \
         if x.v < 0 or x.v > 255 {\n            out(\"BAD\\n\");\n        } else {\n            \
         out(\"OK\\n\");\n        };\n    };\n};\n",
        1,
    );
    assert_eq!(output, "OK\n");
}

#[test]
fn select_expressions_report_an_honest_eval_diagnostic() {
    let result = run(
        &elaborate(
            &parse(
                "extend sys {\n    run() is also {\n        var x: uint = select {\n            \
             50 : min;\n        };\n    };\n};\n",
            )
            .declarations,
        ),
        1,
    );
    assert_eq!(result.diagnostics.len(), 1);
    assert!(
        result.diagnostics[0]
            .render()
            .contains("select distribution expressions are not evaluated yet"),
        "expected the honest select diagnostic: {:?}",
        result.diagnostics[0].render()
    );
}

#[test]
fn emit_feeds_the_temporal_log_and_wait_sees_it() {
    // `emit` records the occurrence in this cycle's state; a temporal
    // wait started in the same cycle succeeds immediately (12.3.2's
    // sub-path over the recorded state).
    let output = clean_output(
        "extend sys {\n    ev: event;\n    run() is also {\n        emit ev;\n        \
         wait @ev;\n        out(\"WAIT DONE\");\n    };\n};\n",
        1,
    );
    assert!(output.contains("WAIT DONE"), "output: {output}");
}

#[test]
fn temporal_wait_sees_events_recorded_by_a_tcm() {
    // A started TCM's numeric wait advanced the cycle and the emission
    // landed in the new cycle's state; the temporal wait `until @ev`
    // (13.2.4's readability-only `until`) succeeds against the log.
    // (An `on` block cannot carry the wait: 11.3 forbids
    // time-consuming actions in on action blocks.)
    let output = clean_output(
        "extend sys {\n    ev: event;\n    emitter() @sys.any is {\n        wait [1]*cycle;\n        \
         emit ev;\n    };\n    run() is also {\n        start emitter();\n        \
         wait until @ev;\n        out(\"RESUMED\");\n    };\n};\n",
        1,
    );
    assert!(output.contains("RESUMED"), "output: {output}");
}

#[test]
fn unsatisfied_temporal_wait_reports_the_budget_guard() {
    let result = run_source(
        "extend sys {\n    run() is also {\n        wait @never_emitted;\n        \
         out(\"UNREACHABLE\");\n    };\n};\n",
        1,
    );
    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec!["E0409"]
    );
    assert!(
        !result.output.contains("UNREACHABLE"),
        "the wait must not complete: {}",
        result.output
    );
}

#[test]
fn failed_expect_rule_runs_the_else_body() {
    // 11.4: `@start => eventually @done` fails when start occurred and
    // done never did; the else body's dut_error reports it.
    let result = run_source(
        "extend sys {\n    start_ev: event;\n    done_ev: event;\n    \
         expect @start_ev => eventually @done_ev else dut_error(\"NEVER DONE\");\n    \
         run() is also {\n            emit start_ev;\n        };\n};\n",
        1,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(
        result.output.contains("NEVER DONE"),
        "output: {}",
        result.output
    );
    assert!(
        result.output.contains("1 DUT error(s)"),
        "output: {}",
        result.output
    );
}

#[test]
fn satisfied_and_vacuous_expect_rules_stay_silent() {
    // The satisfied rule: start then done one cycle later. The vacuous
    // rule: its lhs never occurs, so nothing is obligated (12.2.14).
    let output = clean_output(
        "extend sys {\n    start_ev: event;\n    done_ev: event;\n    other_ev: event;\n    \
         expect @start_ev => eventually @done_ev else dut_error(\"SATISFIABLE\");\n    \
         assume @other_ev => eventually @done_ev else dut_error(\"VACUOUS\");\n    \
         run() is also {\n            emit start_ev;\n            wait [1]*cycle;\n            \
         emit done_ev;\n        };\n};\n",
        1,
    );
    assert!(
        !output.contains("SATISFIABLE") && !output.contains("VACUOUS"),
        "output: {output}"
    );
}

#[test]
fn failed_expect_without_else_body_prints_the_rule() {
    // 11.4: with no dut_error clause the tool prints the rule failure
    // itself, as a check error.
    let result = run_source(
        "extend sys {\n    start_ev: event;\n    done_ev: event;\n    \
         expect @start_ev => eventually @done_ev;\n    run() is also {\n            \
         emit start_ev;\n        };\n};\n",
        1,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(
        result
            .output
            .contains("expect rule failed: temporal property not satisfied"),
        "output: {}",
        result.output
    );
    assert!(
        result.output.contains("1 DUT error(s)"),
        "output: {}",
        result.output
    );
}

#[test]
fn eventually_rule_requires_the_event_before_quit() {
    // `expect eventually @done` is success-required: it fails when the
    // event never occurred anywhere in the run.
    let result = run_source(
        "extend sys {\n    done_ev: event;\n    expect eventually @done_ev else \
         dut_error(\"NO DONE BY QUIT\");\n    run() is also {\n            wait [2]*cycle;\n        \
     };\n};\n",
        1,
    );
    assert!(
        result.output.contains("NO DONE BY QUIT"),
        "output: {}",
        result.output
    );
}

#[test]
fn temporal_wait_spins_until_a_later_cycle_satisfies_the_sequence() {
    // `wait {@a; cycle}` (12.2.9): a occurs in the start cycle; the
    // tail `cycle` holds on the next cycle, so the wait spins once —
    // the padded empty state — and succeeds.
    let output = clean_output(
        "extend sys {\n    a_ev: event;\n    run() is also {\n        emit a_ev;\n        \
         wait {@a_ev; cycle};\n        out(\"SEQ HELD\");\n    };\n};\n",
        1,
    );
    assert!(output.contains("SEQ HELD"), "output: {output}");
}

#[test]
fn proposition_wait_evaluates_with_current_field_values() {
    // `wait true(x == 3)` evaluates the proposition through the
    // interpreter with current field values (12.1.1.1: TRUE or
    // non-zero numeric).
    let output = clean_output(
        "extend sys {\n    x: uint;\n    run() is also {\n        x = 3;\n        \
         wait true(x == 3);\n        out(\"PROP HELD\");\n    };\n};\n",
        1,
    );
    assert!(output.contains("PROP HELD"), "output: {output}");
}

#[test]
fn cover_group_samples_explicit_buckets_on_emit() {
    // 14.2/14.3: the group samples when its event fires; each item
    // value lands in the bucket whose range contains it, and values
    // outside every bucket are ungradeable.
    let result = run_source_with_coverage(
        "extend sys {\n    event cov_e;\n    cur: uint;\n    cover cov_e is {\n        \
         item cur: uint using ranges = {\n            range([0..1], \"low\");\n            \
         range([2..3], \"high\");\n        };\n    };\n    run() is also {\n        \
         cur = 0; emit cov_e;\n        cur = 3; emit cov_e;\n        cur = 7; emit cov_e;\n        \
         cur = 1; emit cov_e;\n    };\n};\n",
        1,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.coverage.len(), 1);
    let groups = &result.coverage[0].groups;
    assert_eq!(groups.len(), 1);
    let group = &groups[0];
    assert_eq!(group.event_path.as_deref(), Some("cov_e"));
    assert_eq!(group.samples, 4);
    let item = &group.items[0];
    assert_eq!(item.name, "cur");
    assert_eq!(item.buckets.len(), 2);
    assert_eq!(item.buckets[0].label, "low");
    assert_eq!(item.buckets[0].hits, 2);
    assert_eq!(item.buckets[1].label, "high");
    assert_eq!(item.buckets[1].hits, 1);
    assert_eq!(item.ungradeable_hits, 1);
    assert_eq!(item.grade(), Some(1.0));
    assert_eq!(group.grade(), Some(1.0));
}

#[test]
fn cover_items_without_ranges_use_the_clause_defaults() {
    // 14.3: integers default to 16 buckets (0 through 15), booleans to
    // their two values; out-of-range values do not affect the grade.
    let result = run_source_with_coverage(
        "extend sys {\n    event cov_e;\n    n: uint;\n    flag: bool;\n    cover cov_e is {\n        \
         item n: uint;\n        item flag: bool;\n    };\n    run() is also {\n        \
         n = 0; flag = TRUE; emit cov_e;\n        n = 15; flag = FALSE; emit cov_e;\n        \
         n = 16; emit cov_e;\n    };\n};\n",
        1,
    );
    let group = &result.coverage[0].groups[0];
    assert_eq!(group.samples, 3);
    let n_item = &group.items[0];
    assert_eq!(n_item.buckets.len(), 16);
    assert_eq!(n_item.buckets[0].hits, 1);
    assert_eq!(n_item.buckets[15].hits, 1);
    assert_eq!(n_item.ungradeable_hits, 1);
    assert_eq!(n_item.grade(), Some(2.0 / 16.0));
    let flag_item = &group.items[1];
    assert_eq!(flag_item.buckets.len(), 2);
    assert_eq!(flag_item.grade(), Some(1.0));
    // The plain average of the two items' grades.
    assert_eq!(group.grade(), Some((2.0 / 16.0 + 1.0) / 2.0));
}

#[test]
fn cover_is_also_merges_items_into_the_base_group() {
    // 14.6: `is also` extensions add items to the base group; an
    // `empty` placeholder receives them too.
    let result = run_source_with_coverage(
        "extend sys {\n    event cov_e;\n    ok: bool;\n    cover cov_e is empty;\n    \
         cover cov_e is also {\n            item ok: bool;\n        };\n    run() is also {\n        \
         ok = TRUE; emit cov_e;\n    };\n};\n",
        1,
    );
    assert_eq!(result.coverage[0].groups.len(), 1);
    let group = &result.coverage[0].groups[0];
    assert_eq!(group.items.len(), 1);
    assert_eq!(group.items[0].name, "ok");
    assert_eq!(group.items[0].buckets[1].hits, 1);
    assert_eq!(group.samples, 1);
}

#[test]
fn untyped_unranged_cover_items_record_without_grading() {
    // An item with no ranges clause and no typed expression is
    // recorded but ungradeable (14.3: goals of 0).
    let result = run_source_with_coverage(
        "extend sys {\n    event cov_e;\n    tag: string;\n    cover cov_e is {\n        \
         item tag;\n        };\n    run() is also {\n        tag = \"a\"; emit cov_e;\n        \
     tag = \"b\"; emit cov_e;\n    };\n};\n",
        1,
    );
    let group = &result.coverage[0].groups[0];
    let item = &group.items[0];
    assert!(item.buckets.is_empty());
    assert_eq!(item.samples, 2);
    assert_eq!(item.ungradeable_hits, 2);
    assert_eq!(item.grade(), None);
    assert_eq!(group.grade(), None);
    assert!(item.note.is_some(), "the honest note is recorded");
}

#[test]
fn select_weights_bias_the_sampler_to_min_and_max() {
    // 10.3.12.3: an arm is chosen with probability weight/(sum of all
    // weights) over the current range; `min`/`max` take the extremes.
    let dominated = |arm_a: &str, arm_b: &str| {
        clean_output(
            &format!(
                "struct s {{\n    v: uint (bits: 8);\n}};\n\
                 extend s {{\n    keep soft v == select {{\n        {arm_a}\n        {arm_b}\n    }};\n}};\n\
                 extend sys {{\n    run() is also {{\n        var x: s;\n        gen x;\n        \
                 print x.v;\n    }};\n}};\n"
            ),
            1,
        )
    };
    assert_eq!(dominated("1000 : min;", "1 : max;"), "0\n");
    assert_eq!(dominated("1 : min;", "1000 : max;"), "255\n");
}

#[test]
fn select_others_draws_strictly_inside_the_range() {
    let output = clean_output(
        "struct s {\n    v: uint (bits: 8);\n};\n\
         extend s {\n    keep soft v == select {\n        100 : others;\n    };\n};\n\
         extend sys {\n    run() is also {\n        var x: s;\n        gen x;\n        \
         print x.v;\n    };\n};\n",
        1,
    );
    // Strictly inside: neither extreme.
    let value: u32 = output.trim().parse().expect("the generated value");
    assert!(value > 0 && value < 255, "output: {output}");
}

#[test]
fn select_edges_picks_one_of_the_extremes() {
    let output = clean_output(
        "struct s {\n    v: uint (bits: 8);\n};\n\
         extend s {\n    keep soft v == select {\n        100 : edges;\n    };\n};\n\
         extend sys {\n    run() is also {\n        var x: s;\n        gen x;\n        \
         print x.v;\n    };\n};\n",
        1,
    );
    assert!(
        output.trim() == "0" || output.trim() == "255",
        "output: {output}"
    );
}

#[test]
fn select_normal_biases_around_its_center() {
    let output = clean_output(
        "struct s {\n    v: uint (bits: 16);\n};\n\
         extend s {\n    keep soft v == select {\n        100 : normal(20000, 1000);\n    };\n};\n\
         extend sys {\n    run() is also {\n        var x: s;\n        gen x;\n        \
         print x.v;\n    };\n};\n",
        1,
    );
    let value: u32 = output.trim().parse().expect("the generated value");
    // 20,000 ± 1,000: five standard deviations around the center —
    // far outside that would mean the normal is not applied.
    assert!((15_000..=25_000).contains(&value), "output: {output}");
}

#[test]
fn select_normal_accepts_max_uint_expressions_like_the_corpus() {
    // The igen-dist corpus shape: normal(MAX_UINT/2, MAX_UINT/10) over
    // an unsized uint (0 ..= 2^32-1). MAX_UINT is a predefined constant
    // of the choice-expression evaluator.
    let output = clean_output(
        "struct s {\n    v: uint;\n};\n\
         extend s {\n    keep soft v == select {\n        100 : normal(MAX_UINT/2, MAX_UINT/10);\n    };\n};\n\
         extend sys {\n    run() is also {\n        var x: s;\n        gen x;\n        \
         print x.v;\n    };\n};\n",
        1,
    );
    let value: u64 = output.trim().parse().expect("the generated value");
    // Center 2^31 ± five standard deviations of 2^32/10.
    assert!(
        value > 100_000_000 && value < 4_200_000_000,
        "output: {output}"
    );
}

#[test]
fn select_choice_on_either_operand_side_biases() {
    // `select { .. } == v` biases the same field.
    let output = clean_output(
        "struct s {\n    v: uint (bits: 8);\n};\n\
         extend s {\n    keep soft select {\n        1000 : max;\n    } == v;\n};\n\
         extend sys {\n    run() is also {\n        var x: s;\n        gen x;\n        \
         print x.v;\n    };\n};\n",
        1,
    );
    assert_eq!(output.trim(), "255");
}

#[test]
fn select_with_an_unevaluable_choice_keeps_the_field_free() {
    // An arm whose choice cannot be evaluated (an unknown name) leaves
    // the domain unwidened — the sampler draws naturally, no error.
    let output = clean_output(
        "struct s {\n    v: uint (bits: 8);\n};\n\
         extend s {\n    keep soft v == select {\n        100 : normal(WAT/2, 1);\n    };\n};\n\
         extend sys {\n    run() is also {\n        var x: s;\n        gen x;\n        \
         if x.v <= 255 {\n            out(\"OK\\n\");\n        };\n    };\n};\n",
        1,
    );
    assert_eq!(output, "OK\n");
}

#[test]
fn when_subtyped_fields_generate_under_their_when_constraints() {
    // 6.7 + 10.3.12.3 together: a when-qualified field (`LO pkt`)
    // generates an instance of the synthesized subtype view, whose
    // when-block constraints — here a select distribution — apply.
    let output = clean_output(
        "type mode_t : [LO, HI];\n\
         struct pkt {\n    kind: mode_t;\n    v: uint (bits: 8);\n    \
         when LO kind is {\n            keep soft v == select { 1000 : min; };\n        };\n    \
         when HI kind is {\n            keep soft v == select { 1000 : max; };\n        };\n};\n\
         extend sys {\n    p_lo: LO pkt;\n    p_hi: HI pkt;\n    run() is also {\n        \
         gen p_lo;\n        gen p_hi;\n        print p_lo.v;\n        print p_hi.v;\n    };\n};\n",
        1,
    );
    assert!(output.contains("0"), "output: {output}");
    assert!(output.contains("255"), "output: {output}");
}

#[test]
fn when_views_carry_methods_and_base_membership() {
    // A method defined in a when block executes in the subtype's
    // context (6.7.2) and resolves through the synthesized view; the
    // view's instances still satisfy `is a` against the base type.
    let output = clean_output(
        "type mode_t : [LO, HI];\n\
         unit u {\n    kind: mode_t;\n    when LO u is {\n        extra: uint (bits: 8);\n        \
         tag() is {\n            out(\"LO SIDE\");\n        };\n    };\n};\n\
         extend sys {\n    lo_u: LO u is instance;\n    run() is also {\n        \
         lo_u.tag();\n        if lo_u is a u {\n            out(\"IS A U\\n\");\n        };\n        \
         print lo_u.extra;\n    };\n};\n",
        1,
    );
    assert!(output.contains("LO SIDE"), "output: {output}");
    assert!(output.contains("IS A U"), "output: {output}");
    assert!(output.contains("0"), "output: {output}");
}

#[test]
fn chained_when_views_compose_both_levels_constraints() {
    // 6.7 chained: a chained-qualified field (`LO HI pkt`) carries the
    // combined view — the outer when's constraint and the when-in-when
    // member's constraint both gate its gen (here two select policies:
    // one field biased to min, the other to max) — and `is a` walks
    // the combined view's containment chain to the base type.
    let output = clean_output(
        "type mode_t : [LO, HI];\n\
         type dir_t : [UP, DOWN];\n\
         struct pkt {\n    kind: mode_t;\n    dir: dir_t;\n    v: uint (bits: 8);\n    w: uint (bits: 8);\n    \
         when LO kind is {\n            keep soft v == select { 1000 : min; };\n        \
         when HI dir is {\n                keep soft w == select { 1000 : max; };\n            };\n        };\n};\n\
         extend sys {\n    p: LO HI pkt;\n    run() is also {\n        gen p;\n        print p.v;\n        print p.w;\n        \
         if p is a pkt {\n            out(\"IS A PKT\");\n        };\n        \
         if p is a LO pkt {\n            out(\"IS A LO PKT\");\n        };\n    };\n};\n",
        1,
    );
    assert!(output.contains("0"), "output: {output}");
    assert!(output.contains("255"), "output: {output}");
    assert!(output.contains("IS A PKT"), "output: {output}");
    assert!(output.contains("IS A LO PKT"), "output: {output}");
}

#[test]
fn template_typed_fields_instantiate_their_monomorphized_view() {
    // 8.3: a field of template-instance type carries the
    // monomorphized struct — its fields generate, its methods resolve,
    // and `is a` accepts the canonical instance name.
    let output = clean_output(
        "struct base_struct {\n    id: int;\n};\n\
         template struct box of <type> like base_struct {\n    payload: <type>;\n    \
         double(): uint is {\n            return payload * 2;\n        };\n};\n\
         extend sys {\n    b: box of (int (bits: 8));\n    run() is also {\n        \
         gen b;\n        b.payload = 21;\n        print b.double();\n        \
         if b is a box of (int (bits: 8)) {\n            out(\"IS THE BOX\\n\");\n        };\n        \
         if b is a base_struct {\n            out(\"IS A BASE\\n\");\n        };\n    };\n};\n",
        1,
    );
    assert!(output.contains("42"), "output: {output}");
    assert!(output.contains("IS THE BOX"), "output: {output}");
    assert!(output.contains("IS A BASE"), "output: {output}");
}

#[test]
fn method_body_new_returns_a_usable_template_instance() {
    // 8.3 with 8.2: a template body's method containing `new iterator
    // of <type>` substitutes the formal in the body's type position,
    // the referenced instance materializes (its only reference is the
    // body — no field or signature names it), and `new` instantiates
    // it: the returned object carries the substituted instance's
    // fields and dispatches its methods across the chain
    // sys → container of (int) → iterator of (int).
    let output = clean_output(
        "struct base_struct {\n    id: int;\n};\n\
         template struct container of <type> like base_struct {\n    \
         make(): base_struct is {\n        return new iterator of <type> with { idx = 41; };\n    };\n};\n\
         template struct iterator of <type> like base_struct {\n    idx: int;\n    \
         bump(): uint is {\n        return idx + 1;\n    };\n};\n\
         extend sys {\n    c: container of (int);\n    run() is also {\n        \
         var it: base_struct;\n        it = c.make();\n        print it.idx;\n        print it.bump();\n    };\n};\n",
        1,
    );
    assert!(output.contains("41"), "output: {output}");
    assert!(output.contains("42"), "output: {output}");
}

#[test]
fn enclosing_when_constraints_gate_gen_of_the_field() {
    // 10.3.6-10.3.7's connected field sets, v1: `gen p` inside a when-view
    // instance is gated by the when block's constraints, rebased from
    // `p.v` to `v` — two sibling instances with different policies
    // produce opposite extremes, like igen-dist's env policies.
    let output = clean_output(
        "type pol_t : [LO, HI];\n\
         struct pkt {\n    v: uint (bits: 8);\n};\n\
         unit env {\n    !p: pkt;\n    fill() is {\n        gen p;\n        \
         if p.v == 0 {\n            out(\"V=0\");\n        };\n        \
         if p.v == 255 {\n            out(\"V=255\");\n        };\n    };\n    when LO env is {\n            \
         keep soft p.v == select { 1000 : min; };\n        };\n    when HI env is {\n            \
         keep soft p.v == select { 1000 : max; };\n        };\n};\n\
         extend sys {\n    lo_env: LO env is instance;\n    hi_env: HI env is instance;\n    \
         run() is also {\n            lo_env.fill();\n            hi_env.fill();\n        };\n};\n",
        1,
    );
    assert!(output.contains("V=0"), "output: {output}");
    assert!(output.contains("V=255"), "output: {output}");
}

#[test]
fn transition_items_collect_value_pairs_from_the_base() {
    // 14.5: the transition derives previous-value/value samples from
    // the base item's consecutive samples; gradeable over the base's
    // bucket pairs.
    let result = run_source_with_coverage(
        "extend sys {\n    event cov_e;\n    cur: uint;\n    cover cov_e is {\n        \
         item cur: uint using ranges = {\n            range([0..1], \"low\");\n            \
         range([2..3], \"high\");\n        };\n        transition cur;\n    };\n    \
         run() is also {\n            cur = 0; emit cov_e;\n            cur = 3; emit cov_e;\n            \
         cur = 0; emit cov_e;\n        };\n};\n",
        1,
    );
    let group = &result.coverage[0].groups[0];
    assert_eq!(group.derived.len(), 1);
    let transition = &group.derived[0];
    assert_eq!(transition.name, "transition__cur");
    // n=3 base samples → n-1=2 transition samples (14.5).
    assert_eq!(transition.samples, 2);
    // low→high and high→low observed of the 4 ordered bucket pairs.
    assert_eq!(transition.combos.len(), 2);
    assert_eq!(transition.total_combos, Some(4));
    assert_eq!(transition.grade(), Some(0.5));
    assert_eq!(group.grade(), Some((1.0 + 0.5) / 2.0));
}

#[test]
fn cross_items_collect_combinations_of_their_sources() {
    // 14.4: the cross shows every combination of the crossed items'
    // values; gradeable over the cross product of the bucket counts.
    let result = run_source_with_coverage(
        "extend sys {\n    event cov_e;\n    a: uint;\n    b: bool;\n    cover cov_e is {\n        \
         item a: uint using ranges = {\n            range([0..1], \"low\");\n            \
         range([2..3], \"high\");\n        };\n        item b: bool;\n        cross a, b;\n    };\n    \
         run() is also {\n            a = 0; b = TRUE; emit cov_e;\n            a = 3; b = FALSE; emit cov_e;\n        };\n};\n",
        1,
    );
    let group = &result.coverage[0].groups[0];
    assert_eq!(group.derived.len(), 1);
    let cross = &group.derived[0];
    assert_eq!(cross.name, "cross_a__b");
    assert_eq!(cross.sources, vec!["a".to_owned(), "b".to_owned()]);
    assert_eq!(cross.samples, 2);
    assert_eq!(cross.combos.len(), 2);
    // 2 buckets × 2 buckets = 4 combinations; 2 observed.
    assert_eq!(cross.total_combos, Some(4));
    assert_eq!(cross.grade(), Some(0.5));
}

#[test]
fn transitions_over_unbucketed_items_count_without_grading() {
    // An unbucketed base (no ranges) records pairs but yields no
    // grade — the honest v1 reading.
    let result = run_source_with_coverage(
        "extend sys {\n    event cov_e;\n    tag: string;\n    cover cov_e is {\n        \
         item tag;\n        transition tag;\n    };\n    run() is also {\n            \
         tag = \"a\"; emit cov_e;\n            tag = \"b\"; emit cov_e;\n        };\n};\n",
        1,
    );
    let group = &result.coverage[0].groups[0];
    let transition = &group.derived[0];
    assert_eq!(transition.samples, 1);
    assert_eq!(transition.combos.len(), 1);
    assert_eq!(transition.total_combos, None);
    assert_eq!(transition.grade(), None);
    assert_eq!(group.grade(), None);
}

#[test]
fn simple_macro_expansion_substitutes_its_argument() {
    // 15.2/15.5/15.6: a define-as macro expands at its invocation
    // sites — the body's parameter references become the matched
    // text, and the result parses as ordinary code. The v1 interpreter
    // has no type objects, so the example passes the type's name as a
    // string actual (the matcher is category-blind, per the
    // documented limit).
    let output = clean_output(
        "type addr_t : int (bits : 8);\n\
         define <e_util_min_value'exp> \"MIN_VALUE[ ]\\(<type>\\)\" as {\n    \
         min_of(<type>);\n};\n\
         extend sys {\n    min_of(t: string): uint is {\n        \
         if t == \"addr_t\" {\n            return 0;\n        };\n        return 1;\n    };\n    \
         run() is also {\n            var lo: uint = MIN_VALUE(\"addr_t\");\n            \
         print lo;\n        };\n};\n",
        1,
    );
    assert!(output.contains("0"), "output: {output}");
}

#[test]
fn macro_expansion_composes_inside_expressions() {
    // The corpus shape: `MIN_VALUE(addr_t)+1..MAX_VALUE(addr_t)-1` —
    // each invocation expands independently inside a larger expression
    // (with string actuals; see the category note above).
    let output = clean_output(
        "type addr_t : int (bits : 8);\n\
         define <e_util_min_value'exp> \"MIN_VALUE[ ]\\(<type>\\)\" as {\n    \
         min_of(<type>);\n};\n\
         define <e_util_max_value'exp> \"MAX_VALUE[ ]\\(<type>\\)\" as {\n    \
         max_of(<type>);\n};\n\
         extend sys {\n    min_of(t: string): int is {\n        return 0;\n    };\n    \
         max_of(t: string): int is {\n        return 200;\n    };\n    run() is also {\n            \
         var span: int = MAX_VALUE(\"addr_t\") - MIN_VALUE(\"addr_t\");\n            \
         print span;\n        };\n};\n",
        1,
    );
    assert!(output.contains("200"), "output: {output}");
}

#[test]
fn macro_argument_with_internal_parens_expands_whole() {
    // An argument containing its own parentheses (`uint (bits : 8)`
    // shaped: a nested call) is consumed whole by the bounded
    // backtracking matcher.
    let output = clean_output(
        "type addr_t : int (bits : 8);\n\
         define <e_util_min_value'exp> \"MIN_VALUE[ ]\\(<type>\\)\" as {\n    \
         min_of(<type>);\n};\n\
         extend sys {\n    min_of(t: string): int is {\n        return 7;\n    };\n    \
         width_of(): string is {\n            return \"addr_t\";\n    };\n    \
         run() is also {\n            var v: int = MIN_VALUE(width_of());\n            \
         print v;\n        };\n};\n",
        1,
    );
    assert!(output.contains("7"), "output: {output}");
}

#[test]
fn later_macro_definitions_have_higher_priority() {
    // 15.5.1: definitions later in the code take priority — the same
    // pattern redefined changes the expansion.
    let output = clean_output(
        "define <pick_low'exp> \"PICK[ ]\\(\\)\" as {\n    out(\"FIRST\");\n};\n\
         define <pick_high'exp> \"PICK[ ]\\(\\)\" as {\n    out(\"SECOND\");\n};\n\
         extend sys {\n    run() is also {\n            PICK();\n        };\n};\n",
        1,
    );
    assert!(output.contains("SECOND"), "output: {output}");
    assert!(!output.contains("FIRST"), "output: {output}");
}

#[test]
fn macro_body_invoking_another_macro_expands() {
    // 15.5.2: the generated code is tested again against the
    // productions — a body invoking another macro expands too.
    let output = clean_output(
        "define <inner'exp> \"INNER[ ]\\(\\)\" as {\n    out(\"INNER RAN\");\n};\n\
         define <outer'exp> \"OUTER[ ]\\(\\)\" as {\n    INNER();\n};\n\
         extend sys {\n    run() is also {\n            OUTER();\n        };\n};\n",
        1,
    );
    assert!(output.contains("INNER RAN"), "output: {output}");
}

#[test]
fn computed_macro_builds_its_expansion_from_the_parameter() {
    // 15.3: the computed body executes at match time and its returned
    // string is the expansion. The computed definition is later than the
    // simple one of the same pattern, so per 15.5.1 it takes priority and
    // its successful evaluation wins; the string it returns is built by
    // concatenating the parameter's text (available as a string, 15.6).
    let output = clean_output(
        "define <csay_low'exp> \"CSAY[ ]\\(<type>\\)\" as {\n    \
         out(\"SIMPLE RAN\");\n};\n\
         define <csay_dac'exp> \"CSAY[ ]\\(<type>\\)\" as computed {\n    \
         return \"out(\" + <type> + \")\";\n};\n\
         extend sys {\n    run() is also {\n            CSAY(\"COMPUTED ONE\");\n        };\n};\n",
        1,
    );
    assert!(output.contains("COMPUTED ONE"), "output: {output}");
    assert!(!output.contains("SIMPLE RAN"), "output: {output}");
}

#[test]
fn computed_macro_reject_match_declines_to_the_simple_definition() {
    // The type-min-max define-as-computed/define-as shape: the later
    // computed definition matches, but its body executes reject_match(),
    // so the definition declines and the matcher continues to the next
    // lower-priority definition (15.5.1's order) — the file's simple
    // define-as of the same pattern applies.
    let output = clean_output(
        "define <dpick_low'exp> \"DPICK[ ]\\(\\)\" as {\n    \
         out(\"SIMPLE APPLIED\");\n};\n\
         define <dpick_dac'exp> \"DPICK[ ]\\(\\)\" as computed {\n    \
         reject_match();\n};\n\
         extend sys {\n    run() is also {\n            DPICK();\n        };\n};\n",
        1,
    );
    assert!(output.contains("SIMPLE APPLIED"), "output: {output}");
}

#[test]
fn computed_macro_with_unknown_routine_declines_to_the_simple_definition() {
    // 15.3: parsing precedes semantic analysis, so the computation
    // cannot presuppose module definitions — an unknown routine fails
    // the evaluation and the definition declines to the simple one
    // (rather than silently producing a wrong expansion).
    let output = clean_output(
        "define <upick_low'exp> \"UPICK[ ]\\(\\)\" as {\n    \
         out(\"SIMPLE UPICK\");\n};\n\
         define <upick_dac'exp> \"UPICK[ ]\\(\\)\" as computed {\n    \
         var the_type := get_type_by_name(\"addr_t\");\n    \
         return \"out(\\\"DAC RAN\\\")\";\n};\n\
         extend sys {\n    run() is also {\n            UPICK();\n        };\n};\n",
        1,
    );
    assert!(output.contains("SIMPLE UPICK"), "output: {output}");
}

#[test]
fn computed_macro_result_variable_is_the_return_value() {
    // 15.3: either the result variable or the return action is used —
    // here the implicit `result` variable holds the expansion string and
    // the body falls off its end.
    let output = clean_output(
        "define <rres'exp> \"RRES[ ]\\(\\)\" as computed {\n    \
         result = \"out(\\\"RESULT RAN\\\")\";\n};\n\
         extend sys {\n    run() is also {\n            RRES();\n        };\n};\n",
        1,
    );
    assert!(output.contains("RESULT RAN"), "output: {output}");
}

#[test]
fn set_of_values_builds_the_legal_value_set() {
    // 5.10.5: the set of all legal values of a scalar type — scalars
    // from their width and signedness, enumerals as their indices. The
    // 2019 reading (the migration plan's C3) drops the 2016 word
    // "numeric": `bool` applies too, and that is what this test pins.
    let output = clean_output(
        "type addr_t : int (bits : 8);\n\
         type color_t : [WHITE, BLACK, RED];\n\
         extend sys {\n    run() is also {\n            print set_of_values(addr_t);\n            \
         print set_of_values(color_t);\n            print set_of_values(bit);\n            \
         print set_of_values(bool);\n        };\n};\n",
        1,
    );
    assert!(output.contains("[-128..127]"), "output: {output}");
    assert!(output.contains("[0..2]"), "output: {output}");
    assert!(output.contains("[0..1]"), "output: {output}");
}

#[test]
fn set_pseudo_methods_query_and_combine() {
    // 28.5: size/uint_size/min/uint_min/max/uint_max on sets, and the
    // union/intersect/diff algebra.
    let output = clean_output(
        "type addr_t : int (bits : 8);\n\
         extend sys {\n    run() is also {\n            var s: set = set_of_values(addr_t);\n            \
         print s.size();\n            print s.min();\n            print s.max();\n            \
         print s.uint_size();\n        };\n};\n",
        1,
    );
    assert!(output.contains("256"), "output: {output}");
    assert!(output.contains("-128"), "output: {output}");
    assert!(output.contains("127"), "output: {output}");
}

#[test]
fn set_union_intersect_diff_canonicalize() {
    // 28.5.1-28.5.3: the algebra results are canonical (5.2.12:
    // sorted, merged, non-neighboring).
    let output = clean_output(
        "extend sys {\n    run() is also {\n            var a: set = set_of_values(bit);\n            \
         var b: set = set_of_values(nibble);\n            print a.union(b);\n            \
         print a.intersect(b);\n            print b.diff(a);\n        };\n};\n",
        1,
    );
    assert!(output.contains("[0..15]"), "output: {output}");
    assert!(output.contains("[0..1]"), "output: {output}");
    assert!(output.contains("[2..15]"), "output: {output}");
}

#[test]
fn min_max_to_string_and_macro_shapes_compose() {
    // The type_min_max example's chain:
    // `set_of_values(<type>).uint_min().as_a(<type>)` — here with a
    // uint-typed set (values ≥ 0) and to_string on the min.
    let output = clean_output(
        "type addr_t : uint (bits : 8);\n\
         extend sys {\n    run() is also {\n            var s: set = set_of_values(addr_t);\n            \
         print s.uint_min();\n            print s.uint_max();\n            \
         print s.uint_size();\n            print s.uint_min().to_string();\n        };\n};\n",
        1,
    );
    assert!(output.contains("0"), "output: {output}");
    assert!(output.contains("255"), "output: {output}");
    assert!(output.contains("256"), "output: {output}");
}

#[test]
fn cast_of_int_to_enumeral_maps_the_index() {
    // 5.10.2: as_a to an enumeral type maps the integer value to the
    // enumerand at that index.
    let output = clean_output(
        "type color_t : [WHITE, BLACK, RED];\n\
         extend sys {\n    run() is also {\n            var n: int = 2;\n            \
         var c: color_t = n.as_a(color_t);\n            print c;\n        };\n};\n",
        1,
    );
    assert!(output.contains("RED"), "output: {output}");
}

#[test]
fn on_event_port_runs_when_the_port_event_is_emitted() {
    // 11.3 + 9.5.2: `on p$` runs its block when the event port's event
    // occurs — here triggered by `emit p$` (the T1 emit trigger; HDL
    // edge/bind-driven triggering stays with the @sim gap). Repeated
    // declarations extend the block (11.3's is-also equivalent).
    let output = clean_output(
        "extend sys {\n    p$: in event_port is instance;\n    on p$ {\n        \
         out(\"PORT ONE\");\n    };\n    on p$ {\n        out(\"PORT TWO\");\n    };\n    \
         run() is also {\n            emit p$;\n        };\n};\n",
        1,
    );
    assert!(output.contains("PORT ONE"), "output: {output}");
    assert!(output.contains("PORT TWO"), "output: {output}");
}

#[test]
fn on_event_port_rejects_time_consuming_actions() {
    // 11.3's restriction: wait/sync/start/compute in an on action
    // block are rejected at check time (the parse result carries the
    // diagnostic; running is not required).
    let parsed = parse(
        "extend sys {\n    p$: in event_port is instance;\n    on p$ {\n        \
         wait cycle;\n    };\n    on p$ {\n        start watcher();\n    };\n};\n",
    );
    assert_eq!(
        parsed
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec!["E0210", "E0210"]
    );
}

#[test]
fn per_instance_items_grade_per_bucket_with_holes() {
    // 14.4.2: each bucket of a per_instance item is its own graded
    // instance (`item==bucket`); unsampled instances are holes; the
    // group grade is the weighted grades of the instances plus the
    // per_type bucket.
    let result = run_source_with_coverage(
        "extend sys {\n    event cov_e;\n    cur: uint;\n    cover cov_e is {\n        \
         item cur: uint using per_instance = TRUE, ranges = {\n            \
         range([0..1], \"low\");\n            range([2..3], \"high\");\n        };\n    };\n    \
         run() is also {\n            cur = 0; emit cov_e;\n            cur = 3; emit cov_e;\n        };\n};\n",
        1,
    );
    let group = &result.coverage[0].groups[0];
    let instances = group.instances();
    assert_eq!(instances.len(), 2);
    assert_eq!(instances[0].name, "cur==low");
    assert_eq!(instances[0].hits, 1);
    assert!(!instances[0].is_hole());
    assert_eq!(instances[0].grade(), 1.0);
    assert_eq!(instances[1].name, "cur==high");
    assert_eq!(instances[1].hits, 1);
    assert_eq!(instances[1].grade(), 1.0);
    // Both instances covered (2/2 weight units) + the per_type bucket
    // (the item's own grade is 2/2 = 1.0): group grade 100%.
    assert_eq!(group.grade(), Some(1.0));
}

#[test]
fn unsampled_per_instance_buckets_are_holes() {
    // 14.4.2: "any instance that is not sampled is marked as a hole" —
    // a hole grades 0 and pulls the group grade down.
    let result = run_source_with_coverage(
        "extend sys {\n    event cov_e;\n    cur: uint;\n    cover cov_e is {\n        \
         item cur: uint using per_instance = TRUE, ranges = {\n            \
         range([0..1], \"low\");\n            range([2..3], \"high\");\n            \
         range([4..5], \"mid\");\n        };\n    };\n    run() is also {\n            \
         cur = 0; emit cov_e;\n        };\n};\n",
        1,
    );
    let group = &result.coverage[0].groups[0];
    let instances = group.instances();
    assert_eq!(instances.len(), 3);
    assert!(instances[0].grade() == 1.0);
    assert!(instances[1].is_hole());
    assert!(instances[2].is_hole());
    // 2 covered weight units out of 3 (low covered, high and mid are
    // holes) + the per_type bucket (the item's own 1/3 grade):
    // (1.0 + 1/3) / 4.
    let expected = (1.0 + 1.0 / 3.0) / 4.0;
    assert!((group.grade().expect("a grade") - expected).abs() < 1e-9);
}

#[test]
fn at_least_sets_the_bucket_goal_and_weight_scales_the_grade() {
    // Table 41's common readings: `at_least = N` is the per-bucket
    // goal, `weight = N` scales the item's grade contribution.
    let result = run_source_with_coverage(
        "extend sys {\n    event cov_e;\n    a: uint;\n    b: bool;\n    cover cov_e is {\n        \
         item a: uint using at_least = 2, ranges = {\n            range([0..1], \"low\");\n        };\n        \
         item b: bool using weight = 3;\n    };\n    run() is also {\n            \
         a = 0; b = TRUE; emit cov_e;\n            a = 1; b = TRUE; emit cov_e;\n        };\n};\n",
        1,
    );
    let group = &result.coverage[0].groups[0];
    // `a`'s single bucket has goal 2 and received 2 hits: covered.
    let a_item = &group.items[0];
    assert_eq!(a_item.buckets[0].goal, 2);
    assert_eq!(a_item.buckets[0].hits, 2);
    assert_eq!(a_item.grade(), Some(1.0));
    // The weighted group grade over the two items: `a` (grade 1.0,
    // weight 1) + `b` (grade 1/2 — the FALSE bucket is a hole —
    // weight 3): (1.0 + 1.5)/4.
    let expected = (1.0 + 0.5 * 3.0) / 4.0;
    assert!((group.grade().expect("a grade") - expected).abs() < 1e-9);
}

#[test]
fn group_level_per_instance_marks_all_items() {
    // The group-level `per_instance = TRUE` spelling marks every item
    // of the group per-instance.
    let result = run_source_with_coverage(
        "extend sys {\n    event cov_e;\n    cur: uint;\n    cover cov_e is {\n        \
         per_instance = TRUE;\n        item cur: uint using ranges = {\n            \
         range([0..1], \"low\");\n        };\n    };\n    run() is also {\n            \
         cur = 0; emit cov_e;\n        };\n};\n",
        1,
    );
    let group = &result.coverage[0].groups[0];
    assert!(group.items[0].per_instance);
    assert_eq!(group.instances().len(), 1);
}

#[test]
fn coverage_mode_off_collects_nothing() {
    // 14.2's NOTE: "Unless coverage mode is turned on first (see 14.9),
    // no coverage results are collected, even if cover groups and cover
    // items are defined." An explicitly disabled builder configuration
    // produces no schema; the runtime starts disabled independently.
    let parsed = parse(
        "extend sys {\n    event cov_e;\n    cur: uint;\n    cover cov_e is {\n        \
         item cur: uint;\n        };\n};\n",
    );
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let elaborated = elaborate(&parsed.declarations);
    assert!(
        elaborated.diagnostics.is_empty(),
        "{:?}",
        elaborated.diagnostics
    );
    let config = e1647::coverage::CoverageConfig {
        mode: false,
        ..e1647::coverage::CoverageConfig::default()
    };
    assert!(e1647::coverage::build_with_config(&elaborated, &config).is_empty());
    // A builder's default constructs a schema. This does not enable
    // runtime collection; set_config controls runtime enablement.
    assert!(
        !e1647::coverage::build_with_config(
            &elaborated,
            &e1647::coverage::CoverageConfig::default()
        )
        .is_empty()
    );
}

#[test]
fn max_int_buckets_configuration_sets_the_default_bucket_count() {
    // 14.3: "If ranges is not specified, the default number of buckets
    // is 16 [set by the max_int_buckets coverage configuration option
    // (see 14.9)]."
    let parsed = parse(
        "extend sys {\n    event cov_e;\n    n: uint;\n    cover cov_e is {\n        item n: uint;\n    };\n};\n",
    );
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let elaborated = elaborate(&parsed.declarations);
    assert!(
        elaborated.diagnostics.is_empty(),
        "{:?}",
        elaborated.diagnostics
    );
    let config = e1647::coverage::CoverageConfig {
        max_int_buckets: 4,
        ..e1647::coverage::CoverageConfig::default()
    };
    let coverage = e1647::coverage::build_with_config(&elaborated, &config);
    let item = &coverage[0].groups[0].items[0];
    assert_eq!(item.buckets.len(), 4);
    assert_eq!(item.buckets[0].label, "0");
    assert_eq!(item.buckets[3].lo, 3);
    // 0 leaves the item without default buckets (ungradeable, noted).
    let config = e1647::coverage::CoverageConfig {
        max_int_buckets: 0,
        ..e1647::coverage::CoverageConfig::default()
    };
    let coverage = e1647::coverage::build_with_config(&elaborated, &config);
    let item = &coverage[0].groups[0].items[0];
    assert!(item.buckets.is_empty());
    assert_eq!(item.grade(), None);
    assert!(item.note.is_some(), "the honest note is recorded");
}

#[test]
fn no_collect_group_option_suppresses_the_group() {
    // 14.3.2: "if one or both of no_collect and instance_no_collect
    // options of the group are evaluated to TRUE, the group shall not
    // be presented under that instance." The constant TRUE spelling is
    // omitted from the reported view and its grade.
    let result = run_source_with_coverage(
        "extend sys {\n    event cov_e;\n    cur: uint;\n    cover cov_e is {\n        \
         no_collect = TRUE;\n        item cur: uint using ranges = {\n            \
         range([0..1], \"low\");\n        };\n    };\n    run() is also {\n        \
         cur = 0; emit cov_e;\n    };\n};\n",
        1,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.coverage.len(), 1);
    assert!(result.coverage[0].groups.is_empty());
}

#[test]
fn instance_no_collect_groups_are_not_presented_either() {
    // 14.3.2's OR relation: instance_no_collect = TRUE alone (no
    // no_collect on the group) suppresses presentation the same way.
    let result = run_source_with_coverage(
        "extend sys {\n    event cov_e;\n    cur: uint;\n    cover cov_e is {\n        \
         per_unit_instance;\n        instance_no_collect = TRUE;\n        item cur: uint;\n    };\n    run() is also {\n        \
         cur = 0; emit cov_e;\n    };\n};\n",
        1,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.coverage[0].groups.is_empty());
}

#[test]
fn no_collect_item_option_suppresses_the_item() {
    // 14.4.4: "if one or both of no_collect and instance_no_collect
    // options of the item are evaluated to TRUE, the item shall not be
    // presented under that instance" — the item drops from the group;
    // its siblings still collect.
    let result = run_source_with_coverage(
        "extend sys {\n    event cov_e;\n    a: uint;\n    b: bool;\n    cover cov_e is {\n        \
         item a: uint using no_collect = TRUE, ranges = {\n            range([0..1], \"low\");\n        };\n        \
         item b: bool;\n    };\n    run() is also {\n            a = 0; b = TRUE; emit cov_e;\n        };\n};\n",
        1,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let group = &result.coverage[0].groups[0];
    assert_eq!(group.items.len(), 1);
    assert_eq!(group.items[0].name, "b");
    assert_eq!(group.items[0].buckets[1].hits, 1);
}

#[test]
fn name_option_renames_cross_and_transition_items() {
    // 14.4/14.5: a cross or transition "creates a new item with a name
    // specified using a name option" instead of the default
    // cross_a__b / transition__base names.
    let result = run_source_with_coverage(
        "extend sys {\n    event cov_e;\n    op: uint;\n    carry: bool;\n    cover cov_e is {\n        \
         item op: uint using ranges = { range([0..1], \"low\"); };\n        \
         item carry: bool;\n        cross op, carry using name = \"op_by_carry\";\n        \
         transition op using name = \"op_seq\";\n    };\n    run() is also {\n            \
         op = 0; carry = TRUE; emit cov_e;\n            op = 1; carry = FALSE; emit cov_e;\n        };\n};\n",
        1,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let group = &result.coverage[0].groups[0];
    assert_eq!(group.derived.len(), 2);
    assert_eq!(group.derived[0].name, "op_by_carry");
    assert_eq!(
        group.derived[0].sources,
        vec!["op".to_owned(), "carry".to_owned()]
    );
    assert_eq!(group.derived[0].combos.len(), 2);
    assert_eq!(group.derived[1].name, "op_seq");
    assert_eq!(group.derived[1].sources, vec!["op".to_owned()]);
    assert_eq!(group.derived[1].samples, 1);
}

#[test]
fn group_level_at_least_and_weight_apply_to_items_without_their_own() {
    // 14.9.3/14.9.5: weights and the at_least minimum "can be specified
    // in the coverage group or item definitions" — the group option
    // applies to every item that does not set its own; the item-level
    // spelling wins where present.
    let result = run_source_with_coverage(
        "extend sys {\n    event cov_e;\n    a: uint;\n    b: bool;\n    c: bool;\n    cover cov_e is {\n        \
         at_least = 2;\n        weight = 3;\n        item a: uint using ranges = {\n            \
         range([0..1], \"low\");\n        };\n        item b: bool using at_least = 4;\n        \
         item c: bool;\n    };\n    run() is also {\n            a = 0; emit cov_e;\n        };\n};\n",
        1,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let group = &result.coverage[0].groups[0];
    // `a` and `c` take the group's readings.
    let a_item = &group.items[0];
    assert_eq!(a_item.buckets[0].goal, 2);
    assert_eq!(a_item.weight, 3);
    assert_eq!(group.items[2].buckets[0].goal, 2);
    assert_eq!(group.items[2].weight, 3);
    // `b`'s own at_least wins; its weight still comes from the group.
    let b_item = &group.items[1];
    assert_eq!(b_item.buckets[0].goal, 4);
    assert_eq!(b_item.weight, 3);
}

#[test]
fn repeat_until_executes_at_least_once() {
    // 20.2.3: the body executes at least once, then repeatedly until
    // the condition is TRUE. (Locals, not fields — the implicit gen
    // randomizes uint fields before run().)
    let output = clean_output(
        "extend sys {\n    run() is also {\n            var i: uint = 0;\n            repeat {\n                \
         i = i + 1;\n            } until i == 3;\n            print i;\n        };\n};\n",
        1,
    );
    assert!(output.contains("3"), "output: {output}");
}

#[test]
fn repeat_until_runs_the_body_before_the_first_check() {
    // The do-while order: a body whose condition is already TRUE at
    // entry still runs once.
    let output = clean_output(
        "extend sys {\n    run() is also {\n            var done: bool = TRUE;\n            \
         var ran: uint = 0;\n            repeat {\n                ran = ran + 1;\n            \
         } until done;\n            print ran;\n        };\n};\n",
        1,
    );
    assert!(output.contains("1"), "output: {output}");
}

#[test]
fn break_exits_a_repeat_until_loop() {
    let output = clean_output(
        "extend sys {\n    run() is also {\n            var i: uint = 0;\n            repeat {\n                \
         i = i + 1;\n                if i == 2 {\n                    break;\n                };\n            \
         } until FALSE;\n            print i;\n        };\n};\n",
        1,
    );
    assert!(output.contains("2"), "output: {output}");
}

#[test]
fn if_do_single_statement_form_parses_and_runs() {
    // 20.1/20.2's optional `do` sugar, one-statement form: `if cond do
    // <statement>;`.
    let output = clean_output(
        "extend sys {\n    run() is also {\n            var i: uint = 3;\n            \
         if i == 3 do out(\"THREE\");\n            if i == 9 do out(\"NINE\");\n        };\n};\n",
        1,
    );
    assert!(output.contains("THREE"), "output: {output}");
    assert!(!output.contains("NINE"), "output: {output}");
}

#[test]
fn while_do_sugar_parses_and_runs() {
    // The iterative constructs' optional `do` (20.2's NOTE 2).
    let output = clean_output(
        "extend sys {\n    run() is also {\n            var i: uint = 0;\n            \
         while i < 3 do {\n                i = i + 1;\n            };\n            print i;\n        };\n};\n",
        1,
    );
    assert!(output.contains("3"), "output: {output}");
}

#[test]
fn define_replacement_substitutes_before_running() {
    // Clause 21.3's replacement rules: the token pre-pass
    // (src/preproc.rs) rewrites the defined name in subsequent code, so
    // the interpreter sees the literal 3.
    let output = clean_output(
        "#define N 3\nextend sys {\n    run() is also {\n        var x: uint = N * 2;\n        \
         out(\"x=\", x, \"\\n\");\n    };\n};\n",
        1,
    );
    assert!(output.contains("x=6"), "output: {output}");
}

#[test]
fn define_replacement_leaves_string_literals_alone() {
    // The substitution stops at string literals (21.3's "except inside
    // string literals").
    let output = clean_output(
        "#define N 3\nextend sys {\n    run() is also {\n        out(\"N=\", \"N\", \"\\n\");\n    };\n};\n",
        1,
    );
    assert!(output.contains("N=N"), "output: {output}");
}

#[test]
fn hash_else_branches_run() {
    // 21.3's `#else`: the alternative group runs when the condition
    // fails, and the primary group runs (the alternative skipped
    // without parsing) when it holds.
    let output = clean_output(
        "extend sys {\n    run() is also {\n        #ifdef MISSING {\n            \
         out(\"no\\n\");\n        } #else {\n            out(\"yes\\n\");\n        };\n    };\n};\n",
        1,
    );
    assert!(output.contains("yes"), "output: {output}");
    assert!(!output.contains("no"), "output: {output}");
    let output = clean_output(
        "extend sys {\n    run() is also {\n        #define MISSING\n        #ifdef MISSING {\n            \
         out(\"flag\\n\");\n        } #else {\n            out(\"other\\n\");\n        };\n    };\n};\n",
        1,
    );
    assert!(output.contains("flag"), "output: {output}");
    assert!(!output.contains("other"), "output: {output}");
}

#[test]
fn sim_rise_event_fires_on_rising_edges_only() {
    // 11.1.4.2: `rise(HDL expression) @sim` monitors the expression each
    // cycle and fires on the rise. The injected signal history stands in
    // for the simulator (the xezim binary is container-only): the value
    // rises in cycles 1 and 4, stays flat in 2, and falls in 3. The
    // cycle-0 sample only establishes the baseline.
    let mut signals = e1647::hdl::InjectedSignals::new();
    signals
        .at("top.clk", 0, 0)
        .at("top.clk", 1, 1)
        .at("top.clk", 2, 1)
        .at("top.clk", 3, 0)
        .at("top.clk", 4, 1);
    let output = run_source_with_signals(
        "extend sys {\n    event tick is rise('top.clk') @sim;\n    on tick {\n        \
         out(\"RISE\\n\");\n    };\n    run() is also {\n        out(\"START\\n\");\n        \
         wait [4]*cycle;\n        out(\"END\\n\");\n    };\n};\n",
        1,
        signals,
    )
    .output;
    assert_eq!(output, "START\nRISE\nRISE\nEND\n");
}

#[test]
fn sim_change_event_fires_on_every_change() {
    // 11.1.4.2: `change(HDL expression) @sim` fires on every value
    // change — cycles 1 and 3 in the injected history; the flat cycle 2
    // does not fire again.
    let mut signals = e1647::hdl::InjectedSignals::new();
    signals
        .at("top.sig", 0, 0)
        .at("top.sig", 1, 1)
        .at("top.sig", 2, 1)
        .at("top.sig", 3, 0);
    let output = run_source_with_signals(
        "extend sys {\n    event flip is change('top.sig') @sim;\n    on flip {\n        \
         out(\"CHANGE\\n\");\n    };\n    run() is also {\n        out(\"START\\n\");\n        \
         wait [3]*cycle;\n        out(\"END\\n\");\n    };\n};\n",
        1,
        signals,
    )
    .output;
    assert_eq!(output, "START\nCHANGE\nCHANGE\nEND\n");
}

#[test]
fn sim_events_never_fire_without_a_simulator() {
    // Without an HDL source there is nothing to sample: the `@sim`
    // event's on block never runs and the run stays clean — the honest
    // "HDL signals without a simulator" behavior.
    let result = run_source(
        "extend sys {\n    event tick is rise('top.clk') @sim;\n    on tick {\n        \
         out(\"RISE\\n\");\n    };\n    run() is also {\n        wait [3]*cycle;\n        \
         out(\"END\\n\");\n    };\n};\n",
        1,
    );
    assert!(result.diagnostics.is_empty());
    assert_eq!(result.output, "END\n");
}

#[test]
fn started_tcm_commences_when_its_sampling_event_occurs() {
    // 17.1.2: a TCM implicitly synchronizes on its designated sampling
    // event upon entry, as if `sync @ev` were added at the top — the
    // `tcm() @ev` TCM below starts only when `ev` fires in cycle 2, not
    // one cycle after its `start` (the watcher's per-cycle prints pin
    // the timing).
    let output = clean_output(
        "extend sys {\n    event ev;\n    tcm() @ev is {\n        out(\"TCM AFTER EV\\n\");\n    };\n    \
         watcher() @sys.any is {\n        var i: uint = 0;\n        while i < 4 {\n            \
         out(\"tick \", i, \"\\n\");\n            wait [1]*cycle;\n            i = i + 1;\n        };\n    };\n    \
         run() is also {\n        start watcher();\n        start tcm();\n        \
         wait [2]*cycle;\n        emit ev;\n    };\n};\n",
        1,
    );
    // The TCM did not commence one cycle after its start (it prints
    // after the watcher's cycle-1 turn) and it ran within ev's cycle 2
    // (before the watcher's cycle-3 turn). Which same-cycle turn goes
    // first is the scheduler's queue order.
    let tick0 = output.find("tick 0").expect("tick 0");
    let fired = output.find("TCM AFTER EV").expect("TCM AFTER EV");
    let tick2 = output.find("tick 2").expect("tick 2");
    assert!(
        tick0 < fired,
        "the TCM starts only when ev occurs: {output}"
    );
    assert!(fired < tick2, "the TCM runs in ev's cycle: {output}");
}

#[test]
fn tcm_with_already_emitted_sampling_event_starts_this_tick() {
    // 17.2.1: a TCM begins "immediately if the sampling event has
    // already been emitted for the current tick" — the emit precedes the
    // start in cycle 0, so the TCM runs before the watcher's first
    // cycle-1 print.
    let output = clean_output(
        "extend sys {\n    event ev;\n    tcm() @ev is {\n        out(\"TCM NOW\\n\");\n    };\n    \
         watcher() @sys.any is {\n        out(\"TICK0\\n\");\n        wait [1]*cycle;\n        \
         out(\"TICK1\\n\");\n    };\n    run() is also {\n        start watcher();\n        \
         emit ev;\n        start tcm();\n        out(\"MAIN\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "MAIN\nTCM NOW\nTICK0\nTICK1\n");
}

#[test]
fn tcm_parked_on_a_never_firing_event_holds_the_run_to_the_budget() {
    // The parked TCM keeps the run alive (13.2.2: it stays idle until
    // the next occurrence of its sampling event); with the event never
    // occurring, the shared cycle budget is what stops the run (E0409).
    let result = run_source(
        "extend sys {\n    ev: event;\n    tcm() @ev is {\n        out(\"NEVER\");\n    };\n    \
         run() is also {\n        start tcm();\n    };\n};\n",
        1,
    );
    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec!["E0409"]
    );
    assert!(
        !result.output.contains("NEVER"),
        "output: {}",
        result.output
    );
}

#[test]
fn bound_event_port_fires_on_its_hdl_object_edge() {
    // 9.5 + 9.7.2.9: the external event port's hdl_path() attribute
    // names the simulated object; the port's event fires on the bound
    // object's edge (the default `change` of 9.7.3) and runs the `on
    // rst$` blocks (11.3) — here driven by the injected history instead
    // of a simulator.
    let mut signals = e1647::hdl::InjectedSignals::new();
    signals
        .at("top.rst", 0, 0)
        .at("top.rst", 1, 1)
        .at("top.rst", 2, 1);
    let output = run_source_with_signals(
        "extend sys {\n    rst$: in event_port is instance;\n    keep rst$.hdl_path() == \"top.rst\";\n    \
         on rst$ {\n        out(\"PORT RST\\n\");\n    };\n    run() is also {\n        \
         out(\"BEGIN\\n\");\n        wait [3]*cycle;\n        out(\"AFTER\\n\");\n    };\n};\n",
        1,
        signals,
    )
    .output;
    assert_eq!(output, "BEGIN\nPORT RST\nAFTER\n");
}

#[test]
fn event_port_edge_attribute_selects_the_edge() {
    // 9.7.3: `keep p.edge() == rise` restricts the port's event to
    // rising edges. The injected history 0,1,0,1 crosses two rises (a
    // plain change edge would fire three times).
    let mut signals = e1647::hdl::InjectedSignals::new();
    signals
        .at("top.sig", 0, 0)
        .at("top.sig", 1, 1)
        .at("top.sig", 2, 0)
        .at("top.sig", 3, 1);
    let output = run_source_with_signals(
        "extend sys {\n    p$: in event_port is instance;\n    keep p$.hdl_path() == \"top.sig\";\n    \
         keep p$.edge() == rise;\n    on p$ {\n        out(\"EDGE\\n\");\n    };\n    \
         run() is also {\n        wait [4]*cycle;\n        out(\"DONE\\n\");\n    };\n};\n",
        1,
        signals,
    )
    .output;
    assert_eq!(output, "EDGE\nEDGE\nDONE\n");
}

#[test]
fn brace_grouped_members_generate_and_run() {
    // 4.3.2 / 4.3.3: grouped statements and members are pure syntax —
    // the generated struct behaves exactly as if its members were
    // written flat. The grouped `keep a < 10` holds on gen (the
    // comparison narrows the sampling domain), so `out("ok")` runs.
    let output = clean_output(include_str!("fixtures/s2019_brace_groups.e"), 1);
    assert_eq!(output, "ok");
}

#[test]
fn comparison_constraints_narrow_generation() {
    // A constant comparison in a `keep` narrows the field's sampling
    // domain (`a < 10` directly, `0 > b` mirrored), so gen succeeds
    // where plain rejection sampling over an unbounded scalar would
    // exhaust its attempts. The drawn values still satisfy the
    // constraints as written.
    let output = clean_output(
        "struct s {\n    a: uint;\n    keep a < 10;\n    b: int;\n    keep 0 > b;\n};\n\
         extend sys {\n    run() is also {\n        var x: s;\n        gen x;\n        \
         out(x.a < 10, \" \", x.b < 0, \"\\n\");\n    };\n};\n",
        7,
    );
    assert_eq!(output, "TRUE TRUE\n");
}

#[test]
fn in_table_keep_constrains_gen_and_direct_checks_decide() {
    // 4.11.8: a `keep (a, b, c) in_table { rows }` constrains the tuple
    // to the rows — every generated instance must match at least one
    // row — and the same construct decides directly as an expression
    // (with `not` negating).
    let output = clean_output(
        "type speed_t: [LOW, HIGH, FULL];\n\
         type kind_t: [DATA, INTERRUPT, CONTROL];\n\
         struct pipe {\n\
         \x20   kind: kind_t;\n\
         \x20   speed: speed_t;\n\
         \x20   size: uint;\n\
         \x20   keep (kind, speed, size) in_table {\n\
         \x20       INTERRUPT, FULL, [0..300];\n\
         \x20       CONTROL,   HIGH, 100;\n\
         \x20       DATA,      LOW,  1;\n\
         \x20   };\n\
         };\n\
         extend sys {\n\
         \x20   run() is also {\n\
         \x20       var p: pipe;\n\
         \x20       for i from 1 to 20 {\n\
         \x20           gen p;\n\
         \x20           if (p.kind, p.speed, p.size) not in_table {\n\
         \x20               INTERRUPT, FULL, [0..300];\n\
         \x20               CONTROL,   HIGH, 100;\n\
         \x20               DATA,      LOW,  1;\n\
         \x20           } then { out(\"BAD\") };\n\
         \x20       };\n\
         \x20       out((DATA, LOW, 1) in_table { DATA, LOW, 1 });\n\
         \x20       out((DATA, LOW, 2) in_table { DATA, LOW, 1 });\n\
         \x20       out((DATA, LOW, 2) not in_table { DATA, LOW, 1 });\n\
         \x20   };\n\
         };\n",
        1,
    );
    assert!(
        !output.contains("BAD"),
        "every generated instance matches a row: {output}"
    );
    assert!(output.ends_with("TRUEFALSETRUE"), "output: {output}");
}

#[test]
fn in_table_set_row_elements_match_by_membership() {
    // 4.11.8: a set row element matches by `in` — a range literal, an
    // item list, and a set_of_values type set; a scalar element by `==`.
    let output = clean_output(
        "extend sys {\n\
         \x20   run() is also {\n\
         \x20       out((5) in_table { [1, 5, 9]; });\n\
         \x20       out((5) in_table { [0..4]; });\n\
         \x20       out((5) in_table { [0..300]; });\n\
         \x20       out((5) in_table { set_of_values(uint(bits: 3)); });\n\
         \x20       out((TRUE) in_table { [TRUE, FALSE]; });\n\
         \x20   };\n\
         };\n",
        1,
    );
    assert_eq!(output, "TRUEFALSETRUETRUETRUE");
}

#[test]
fn in_table_element_type_mismatch_reports_e0412() {
    // A tuple element and a row element of incompatible kinds — an
    // integer against a string — report E0412 at run time.
    let result = run_source(
        "extend sys {\n    run() is also {\n        out((1, \"a\") in_table { 1, 2; });\n    };\n};\n",
        1,
    );
    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec!["E0412"]
    );
}

#[test]
fn evaluate_expression_yields_the_block_variable() {
    // 4.17.7: the actions run; the implicit `value` (or the named
    // variable) after the block is the expression's value.
    let output = clean_output(
        "extend sys {\n\
         \x20   run() is also {\n\
         \x20       var x: int = evaluate { value = 3; value = value * 4 };\n\
         \x20       var y: uint = evaluate uint (acc) { acc = 1; for i from 1 to 4 { acc = acc * 2 } };\n\
         \x20       out(x); out(y);\n\
         \x20   };\n\
         };\n",
        1,
    );
    assert_eq!(output, "1216");
}

#[test]
fn evaluate_infers_from_assignment_and_var_forms() {
    // 4.17.7 type inference v1: the declared type of the assignment
    // target, and the `(var)` form without a declared type.
    let output = clean_output(
        "extend sys {\n\
         \x20   run() is also {\n\
         \x20       var z: int;\n\
         \x20       z = evaluate { value = 7 };\n\
         \x20       out(z);\n\
         \x20       var w: int = evaluate (v) { v = 1 + 1 };\n\
         \x20       out(w);\n\
         \x20   };\n\
         };\n",
        1,
    );
    assert_eq!(output, "72");
}

#[test]
fn evaluate_without_an_inferable_type_reports_e0122() {
    // 4.17.7: an untyped evaluate with no assignment/var target and no
    // parameter context cannot infer its result type (E0122).
    let result = run_source(
        "extend sys {\n    run() is also {\n        out(evaluate { value = 1; });\n    };\n};\n",
        1,
    );
    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec!["E0122"]
    );
}

#[test]
fn typeof_and_typeof_item_resolve_in_type_positions() {
    // 5.9: `typeof_item(data)` is the element type of the list field
    // (`byte`), `typeof(first_item)` chains to the same static type —
    // the expression is never evaluated, the resolved type substitutes
    // wherever the type position is used. `x.n` therefore behaves as a
    // byte (200 + 55 wraps to 255) and `typeof(x.n)` types a local var.
    let output = clean_output(
        "struct s {\n    data: list of byte;\n    first_item: typeof_item(data);\n    n: typeof(first_item);\n};\n\
         extend sys {\n    run() is also {\n        var x: s = new;\n        x.first_item = 200;\n        \
         x.n = x.first_item + 55;\n        out(x.n, \"\\n\");\n        var k: typeof(x.n) = 7;\n        \
         out(k, \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "255\n7\n");
}

#[test]
fn typeof_resolves_through_local_vars_and_literals() {
    // 5.9: a local var's declared type and a literal's scalar base are
    // the static types typeof() resolves against; `typeof(x.first_item)`
    // walks a member path through a struct-typed local, and a literal
    // argument types a list's element.
    let output = clean_output(
        "struct s {\n    data: list of byte;\n    first_item: typeof_item(data);\n};\n\
         extend sys {\n    run() is also {\n        var x: s = new;\n        var k: typeof(x.first_item) = 3;\n        \
         var l: list of typeof(k);\n        l.push(k * 2);\n        out(l[0], \"\\n\");\n        \
         var m: typeof(5) = 9;\n        out(m + 1, \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "6\n10\n");
}

#[test]
fn qualified_field_access_reads_when_subtype_fields() {
    // 4.17.3: a subtype-qualified access reads the field declared in the
    // matching when subtype — with short qualifiers (`red'good`), full
    // ones (`TRUE'good`, `red'color`), order-independent words
    // (`green'color'big'size`), and qualifiers already implied by the
    // static type omitted (`red'color` on a `good packet`, `big'green`).
    // The instance must currently match the subtype; the sys keeps fix
    // the determinants before run() reads them.
    let output = clean_output(
        "type color_t: [red, green, blue];\n\
         type size_t: [big, medium, small];\n\
         struct packet {\n    color: color_t;\n    size: size_t;\n    good: bool;\n    \
         when red good packet { x: int; keep x == 5; };\n    \
         when big'size green'color packet { y: uint; keep y == 9; };\n};\n\
         extend sys {\n    p1: good packet;  keep p1.color == red;\n    \
         p2: packet;       keep p2.color == green; keep p2.size == big;\n    \
         run() is also {\n        out(p1.red'good'x, \"\\n\");\n        out(p1.TRUE'good'red'x, \"\\n\");\n        \
         out(p1.red'color'x, \"\\n\");\n        out(p2.green'color'big'size'y, \"\\n\");\n        \
         out(p2.big'green'y, \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "5\n5\n5\n9\n9\n");
}

#[test]
fn qualified_field_access_on_non_matching_subtype_reports_e0413() {
    // 4.17.3 at run time: p2's color is green (its keep), so it is not a
    // `red good packet` and the field x is not present — E0413 names the
    // field and the subtype.
    let result = run_source(
        "type color_t: [red, green, blue];\n\
         struct packet {\n    color: color_t;\n    good: bool;\n    \
         when red good packet { x: int; keep x == 5; };\n};\n\
         extend sys {\n    p2: packet; keep p2.color == green;\n    \
         run() is also {\n        out(p2.red'good'x, \"\\n\");\n    };\n};\n",
        1,
    );
    let diagnostic = &result.diagnostics[0];
    assert_eq!(diagnostic.code, "E0413");
    assert!(
        diagnostic
            .render()
            .contains("field 'x' is not present: instance is not of subtype 'red good packet'"),
        "rendered: {}",
        diagnostic.render()
    );
}

#[test]
fn qualified_field_access_writes_and_checks_presence() {
    // 4.17.3: a write through a subtype-qualified access lands only when
    // the instance currently matches the subtype, and `is a` still
    // dispatches on the plain base type.
    let output = clean_output(
        "type color_t: [red, green, blue];\n\
         struct packet {\n    color: color_t;\n    good: bool;\n    \
         when red good packet { x: int; keep x == 5; };\n};\n\
         extend sys {\n    p1: good packet; keep p1.color == red;\n    \
         run() is also {\n        p1.red'good'x = 41;\n        out(p1.red'good'x + 1, \"\\n\");\n        \
         if p1 is a packet {\n            out(\"IS A PACKET\\n\");\n        };\n    };\n};\n",
        1,
    );
    assert_eq!(output, "42\nIS A PACKET\n");
}

#[test]
fn table1_phase_order_setup_run_extract_check_finalize() {
    // T8.1 (Table 1, 28.3.3.6-28.3.3.8, 28.3.5.1): the extract, check
    // and finalize phases call the same-named method on every live
    // instance after the scheduler drains, in the field-tree DFS order
    // run() uses; setup() runs before generation. The per-struct
    // extract()/check() naming is the 2019 reading (the plan's C11);
    // the 2016 Table 1 named only the sys-level hooks.
    let output = clean_output(
        "struct leaf { run() is also { out(\"leaf run\\n\"); }; \
         extract() is also { out(\"leaf extract\\n\"); }; \
         check() is also { out(\"leaf check\\n\"); }; \
         finalize() is also { out(\"leaf finalize\\n\"); }; };\n\
         extend sys {\n    l: leaf;\n    \
         setup() is also { out(\"setup\\n\"); };\n    \
         run() is also { out(\"sys run\\n\"); };\n    \
         extract() is also { out(\"sys extract\\n\"); };\n    \
         check() is also { out(\"sys check\\n\"); };\n    \
         finalize() is also { out(\"sys finalize\\n\"); };\n};\n",
        1,
    );
    assert_eq!(
        output,
        "setup\n\
         sys run\n\
         leaf run\n\
         sys extract\n\
         leaf extract\n\
         sys check\n\
         leaf check\n\
         sys finalize\n\
         leaf finalize\n"
    );
}

#[test]
fn generation_subphases_run_depth_first_after_post_generate() {
    // T8.2 (9.13.2-9.13.4): after generation and every post_generate(),
    // connect_pointers(), then connect_ports(), then check_generation()
    // run over the unit tree depth-first in field order; run() comes
    // after all of them.
    let output = clean_output(
        "unit u {\n    \
         connect_pointers() is also { out(\"cp \", e_path(), \"\\n\"); };\n    \
         connect_ports() is also { out(\"cpt \", e_path(), \"\\n\"); };\n    \
         check_generation() is also { out(\"cg \", e_path(), \"\\n\"); };\n    \
         post_generate() is also { out(\"pg \", e_path(), \"\\n\"); };\n};\n\
         extend sys {\n    a: u;\n    b: u;\n    \
         run() is also { out(\"run starts\\n\"); };\n};\n",
        1,
    );
    assert_eq!(
        output,
        "pg sys.a\n\
         pg sys.b\n\
         cp sys.a\n\
         cp sys.b\n\
         cpt sys.a\n\
         cpt sys.b\n\
         cg sys.a\n\
         cg sys.b\n\
         run starts\n"
    );
}

#[test]
fn do_bind_outside_connect_ports_is_e0420() {
    // T8.2: do_bind() may only run in the connect_ports() sub-phase;
    // anywhere else it reports E0420 (9.13.3).
    let result = run_source("extend sys {\n    run() is also { do_bind(); };\n};\n", 1);
    let diagnostic = &result.diagnostics[0];
    assert_eq!(diagnostic.code, "E0420");
    assert!(
        diagnostic
            .render()
            .contains("do_bind() may only be called in the connect_ports() sub-phase"),
        "rendered: {}",
        diagnostic.render()
    );
}

#[test]
fn interface_dispatch_casts_and_is_a_compose() {
    // 5.2.9/5.4.4.4/5.7.1/5.10.2.4 (the migration's T3.3): an
    // interface-typed variable holds the implementing instance; a call
    // through it dispatches on the object's actual type; `is a` holds
    // for implementors; an `as_a` between an interface and an unrelated
    // struct is legal and yields NULL on mismatch.
    let output = clean_output(
        "interface shape { area(): uint; };\n\
         struct square implementing shape { side: uint; area(): uint is { return side * side; }; };\n\
         struct rect implementing shape { w: uint; h: uint; area(): uint is { return w * h; }; };\n\
         struct blob { };\n\
         extend sys {\n    run() is also {\n        var sq: square = new; sq.side = 3;\n        \
         var rc: rect = new; rc.w = 2; rc.h = 5;\n        \
         var shapes: list of shape = {sq; rc};\n        \
         var total: uint = 0;\n        \
         for each (s) in shapes { total = total + s.area(); };\n        \
         out(total, \"\n\");\n        \
         var sh: shape = sq;\n        \
         out(sh is a square, \"\n\");\n        \
         out(sh.as_a(rect) == NULL, \"\n\");\n        \
         var b: blob = new;\n        \
         out(b is a shape, \"\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "19\nTRUE\nTRUE\nFALSE\n");
}

#[test]
fn interface_base_assignment_and_casts_follow_inheritance() {
    // 6.13 + 5.4.4.4: implementing an interface implies implementing its
    // `like` bases — a struct→base-interface assignment is the automatic
    // cast, an interface→base-interface cast keeps the pointer, and a
    // base→unrelated-interface cast is legal but NULL.
    let output = clean_output(
        "interface base_i { m(): uint; };\n\
         interface derived_i like base_i { n(): uint; };\n\
         interface unrelated { m(): uint; };\n\
         struct s implementing derived_i {\n    m(): uint is { return 7; };\n    n(): uint is { return 8; };\n};\n\
         extend sys {\n    run() is also {\n        var d: derived_i = new s;\n        \
         out(d.n(), \"\n\");\n        \
         var b: base_i = d;\n        \
         out(b.m(), \"\n\");\n        \
         out(b.as_a(derived_i).n(), \"\n\");\n        \
         var u: unrelated = b.as_a(unrelated);\n        \
         out(u == NULL, \"\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "8\n7\n8\nTRUE\n");
}

#[test]
fn interface_call_without_implementation_reports_e0414() {
    // 6.13 (the migration's T3.4): the interface method resolves through
    // the object's actual struct type; a type that implements the
    // interface without providing the method is a run-time error at the
    // call — E0414 names the struct and the method.
    let result = run_source(
        "interface shape { area(): uint; };\n\
         struct lazy implementing shape { };\n\
         extend sys {\n    run() is also {\n        var s: shape = new lazy;\n        \
         s.area();\n    };\n};\n",
        1,
    );
    let diagnostic = &result.diagnostics[0];
    assert_eq!(diagnostic.code, "E0414");
    assert!(
        diagnostic
            .render()
            .contains("struct 'lazy' does not implement interface method 'area'"),
        "rendered: {}",
        diagnostic.render()
    );
}

#[test]
fn do_bind_inside_connect_ports_is_accepted() {
    // T8.2: inside the connect_ports() sub-phase the call is legal; the
    // keep-bind model has no procedural bindings pending, so it is a
    // no-op there.
    let output = clean_output(
        "extend sys {\n    \
         connect_ports() is also { do_bind(); out(\"bound\\n\"); };\n    \
         run() is also { out(\"run\\n\"); };\n};\n",
        1,
    );
    assert_eq!(output, "bound\nrun\n");
}

#[test]
fn port_bound_set_and_is_connected_over_keep_bind() {
    // T8.3 (9.13.5-9.13.12): two simple ports bound with
    // `keep bind(p, q)` form a two-member bound set (the port itself
    // included) and are connected; a port bound to nothing on purpose
    // (`keep bind(r, empty)`) has only itself and is not connected.
    let output = clean_output(
        "extend sys {\n    \
         p: inout simple_port of bit is instance;\n    \
         q: inout simple_port of bit is instance;\n    \
         r: inout simple_port of bit is instance;\n    \
         keep bind(p, q);\n    keep bind(r, empty);\n    \
         run() is also {\n        \
         out(p.get_bound_set().size(), \"\\n\");\n        \
         out(p.is_connected(), \"\\n\");\n        \
         out(r.get_bound_set().size(), \"\\n\");\n        \
         out(r.is_connected(), \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "2\nTRUE\n1\nFALSE\n");
}

#[test]
fn port_disconnect_clears_the_bound_set() {
    // T8.3: disconnect() drops every connection of the port (its bound
    // set becomes the port itself, is_connected() FALSE) while the
    // former partner also loses the edge.
    let output = clean_output(
        "extend sys {\n    \
         p: inout simple_port of bit is instance;\n    \
         q: inout simple_port of bit is instance;\n    \
         keep bind(p, q);\n    \
         run() is also {\n        \
         p.disconnect();\n        \
         out(p.get_bound_set().size(), \" \", p.is_connected(), \"\\n\");\n        \
         out(q.get_bound_set().size(), \" \", q.is_connected(), \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "1 FALSE\n1 FALSE\n");
}

#[test]
fn port_connect_and_remap_hdl_path_are_e0419() {
    // T8.3: the method-port connection and HDL-path remapping calls are
    // recognised (9.13.5-9.13.12) but not implemented in this stage.
    let result = run_source(
        "extend sys {\n    \
         p: inout simple_port of bit is instance;\n    \
         run() is also { p.connect(p); };\n};\n",
        1,
    );
    assert_eq!(result.diagnostics[0].code, "E0419");
    assert!(result.diagnostics[0].render().contains("'connect()'"));
    let result = run_source(
        "extend sys {\n    \
         p: inout simple_port of bit is instance;\n    \
         run() is also { p.remap_hdl_path(\"sig\"); };\n};\n",
        1,
    );
    assert_eq!(result.diagnostics[0].code, "E0419");
    assert!(
        result.diagnostics[0]
            .render()
            .contains("'remap_hdl_path()'")
    );
}

#[test]
fn get_children_lists_unit_instances_in_field_order() {
    // T8.4 (7.4.4): get_children() returns the direct child unit
    // instances in instance-field declaration order, lists flattened,
    // NULLs skipped — here u1, u2[0], u2[1] (three children), whose
    // e_path() spellings follow that order (7.4).
    let output = clean_output(
        "unit u {\n};\n\
         extend sys {\n    u1: u;\n    u2: list of u;\n    keep u2.size() == 2;\n    \
         run() is also {\n        \
         out(sys.get_children().size(), \"\\n\");\n        \
         for each (c) in sys.get_children() {\n            \
         out(c.e_path(), \" \");\n        };\n        \
         out(\"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "3\nsys.u1 sys.u2[0] sys.u2[1] \n");
}

#[test]
fn get_ports_and_get_ports_recursively_return_port_handles() {
    // T8.4 (7.4.5-7.4.7): get_ports() lists the unit's own ports;
    // get_ports_recursively() adds every sub-instance's; a port handle
    // (with_ports TRUE on get_children) is also iterable.
    let output = clean_output(
        "unit u {\n    q: inout simple_port of bit is instance;\n};\n\
         extend sys {\n    sub: u;\n    p: inout simple_port of bit is instance;\n    \
         run() is also {\n        \
         out(sys.get_ports().size(), \"\\n\");\n        \
         out(sys.get_ports_recursively().size(), \"\\n\");\n        \
         out(sys.get_children(TRUE).size(), \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "1\n2\n2\n");
}

#[test]
fn set_check_accepts_name_and_effect_and_optional_root_unit() {
    // T8.5 (16.3.6): set_check(name, severity|effect [, root_unit])
    // parses and arity-checks; the check-effect model is a later
    // milestone so the accepted call has no effect. set_check_by_name
    // (16.3.7) is an E0419 stub.
    let output = clean_output(
        "extend sys {\n    \
         run() is also {\n            \
         set_check(\"chk\", LOW);\n            \
         set_check(\"chk\", LOW, sys);\n            \
         out(\"checked\\n\");\n        };\n};\n",
        1,
    );
    assert_eq!(output, "checked\n");
    let result = run_source(
        "extend sys {\n    run() is also { set_check_by_name(\"c\", LOW); };\n};\n",
        1,
    );
    assert_eq!(result.diagnostics[0].code, "E0419");
    assert!(
        result.diagnostics[0]
            .render()
            .contains("'set_check_by_name()' is recognised (clause 16.3.7)")
    );
}

#[test]
fn numeric_list_aggregates_accept_real_and_exp_forms() {
    // T8.6 (27.4.17, 27.4.20, 27.5.4-27.5.6): sum/product/average/
    // max_value/min_value take any numeric item expression (reals
    // included) and return the expression's type; integer average
    // truncates (keeps the int type); empty product is 1, empty sum 0.
    // Differential reading (the plan's C4): the 2016 clauses scoped
    // these to integer lists; the 2019 text makes them numeric, and
    // the real sums below pin that reading.
    let output = clean_output(
        "extend sys {\n    run() is also {\n        \
         out({1.5; 2.5}.sum(it), \"\\n\");\n        \
         out({1; 2; 4}.average(it), \"\\n\");\n        \
         out({}.product(it), \"\\n\");\n        \
         out({2; 3; 5}.product(it), \"\\n\");\n        \
         out({}.sum(it), \"\\n\");\n        \
         out({3; 1; 2}.max_value(it), \"\\n\");\n        \
         out({3; 1; 2}.min_value(it), \"\\n\");\n        \
         out({1.5; 2.0}.average(it), \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "4.0\n2\n1\n30\n0\n3\n1\n1.75\n");
}

#[test]
fn average_of_empty_list_is_an_honest_error() {
    // T8.6: the standard's empty-list average is UNDEF, which the value
    // model has no representation for — the run reports E0402 instead
    // of inventing a value.
    let result = run_source(
        "extend sys {\n    run() is also { out({}.average(it)); };\n};\n",
        1,
    );
    let diagnostic = &result.diagnostics[0];
    assert_eq!(diagnostic.code, "E0402");
    assert!(
        diagnostic.render().contains("average of an empty list"),
        "rendered: {}",
        diagnostic.render()
    );
}

#[test]
fn deep_copy_mints_a_fresh_list_and_copies_values() {
    // T8.7 (29.2.2): a list deep copy returns a NEW list object whose
    // items are deep copies — the original is untouched by later
    // modifications of the copy; scalars copy as themselves. 29.2.2's
    // 2019 generalisation (the plan's C7) extends the routine to
    // scalars/strings/sets/lists; the scalar arm below pins it.
    let output = clean_output(
        "extend sys {\n    run() is also {\n        \
         var l1: list of int = {1; 2; 3; 4};\n        \
         var l2 := deep_copy(l1);\n        \
         l2.add(9);\n        \
         out(l1.size(), \" \", l2.size(), \"\\n\");\n        \
         out(deep_copy(42), \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "4 5\n42\n");
}

#[test]
fn deep_copy_of_a_struct_is_a_fresh_instance() {
    // T8.7: a struct deep copy is a new instance of the same type with
    // deep-copied fields; the original is untouched (29.2.2).
    let output = clean_output(
        "struct cell {\n    v: int;\n};\n\
         extend sys {\n    run() is also {\n        \
         var c: cell;\n        c.v = 7;\n        \
         var d := deep_copy(c);\n        \
         d.v = 9;\n        \
         out(c.v, \" \", d.v, \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "7 9\n");
}

#[test]
fn files_flush_log_file_is_e0419() {
    // T8.8 (30.5.16): the files singleton is reachable as a method
    // receiver; flush_log_file() is recognised but not implemented.
    let result = run_source(
        "extend sys {\n    run() is also { files.flush_log_file(); };\n};\n",
        1,
    );
    let diagnostic = &result.diagnostics[0];
    assert_eq!(diagnostic.code, "E0419");
    assert!(
        diagnostic
            .render()
            .contains("'flush_log_file()' is recognised (clause 30.5.16)"),
        "rendered: {}",
        diagnostic.render()
    );
}

#[test]
fn covers_sample_cg_samples_without_emitting_the_event() {
    // T8.9 (14.8.2.10): covers.sample_cg("type.group") samples the
    // group procedurally — three calls after assigning 0, 1, 2, with
    // the sampling event never emitted, hit 3 of the 4 buckets of the
    // uint(bits: 2) item.
    let result = run_source_with_coverage(
        "extend sys {\n    \
         event cg;\n    x: uint(bits: 2);\n    \
         cover cg is {\n            item x: uint(bits: 2);\n    };\n    \
         run() is also {\n        \
         x = 0;\n        covers.sample_cg(\"sys.cg\");\n        \
         x = 1;\n        covers.sample_cg(\"sys.cg\");\n        \
         x = 2;\n        covers.sample_cg(\"sys.cg\");\n    };\n};\n",
        1,
    );
    assert!(
        result.diagnostics.is_empty(),
        "diagnostics: {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
    let entry = result
        .coverage
        .iter()
        .find(|entry| entry.type_name == "sys")
        .expect("sys coverage");
    let group = &entry.groups[0];
    assert_eq!(group.samples, 3, "three procedural samples");
    let item = &group.items[0];
    assert_eq!(item.buckets.len(), 4, "uint(bits: 2) has four buckets");
    let hit = item
        .buckets
        .iter()
        .filter(|bucket| bucket.hits >= bucket.goal)
        .count();
    assert_eq!(hit, 3, "buckets 0, 1, 2 hit");
    assert_eq!(item.buckets[3].hits, 0, "bucket 3 untouched");
}

#[test]
fn covers_sample_cg_is_not_permitted_before_post_generate() {
    // T8.9: the procedural sampling is not permitted before the
    // post_generate() sub-phase ran (14.8.2.10) — the setup phase runs
    // earlier, so a call there reports.
    let result = run_source(
        "extend sys {\n    \
         event cg;\n    cover cg is { item n: uint; };\n    \
         setup() is also { covers.sample_cg(\"sys.cg\"); };\n};\n",
        1,
    );
    let diagnostic = &result.diagnostics[0];
    assert_eq!(diagnostic.code, "E0402");
    assert!(
        diagnostic
            .render()
            .contains("not permitted before the post_generate() sub-phase"),
        "rendered: {}",
        diagnostic.render()
    );
}

#[test]
fn covers_compute_and_read_the_overall_grade() {
    // T8.9 (14.9.6): compute_overall_grade() computes and stores the
    // normalised grade (the covered-bucket fraction over the collected
    // groups, 0.0-1.0); get_overall_grade() reads the last computed
    // value. get_ucd_path() (14.9.15-14.9.17) is the empty string — no
    // coverage database is written in this stage.
    let result = run_source_with_coverage(
        "extend sys {\n    \
         event cg;\n    ok: bool;\n    cover cg is { item ok: bool; };\n    \
         run() is also {\n            \
         ok = TRUE;\n            emit cg;\n            \
         ok = FALSE;\n            emit cg;\n            \
         out(covers.compute_overall_grade(), \"\\n\");\n            \
         out(covers.get_overall_grade(), \"\\n\");\n            \
         out(covers.get_ucd_path() == \"\", \"\\n\");\n        };\n};\n",
        1,
    );
    assert!(
        result.diagnostics.is_empty(),
        "diagnostics: {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
    assert_eq!(result.output, "1.0\n1.0\nTRUE\n");
}

#[test]
fn covers_configuration_calls_are_e0419() {
    // T8.9 (14.9.15-14.9.17): the coverage-configuration calls are
    // recognised stubs.
    for call in [
        "covers.set_per_inst_for_struct(sys, sys)",
        "covers.set_unicov_hier(sys, \"u0\")",
        "covers.set_cross_instance_collection(n, \"p\", TRUE)",
        "covers.get_top_size_items(3)",
    ] {
        let source =
            format!("extend sys {{\n    n: uint;\n    run() is also {{ {call}; }};\n}};\n");
        let result = run_source(&source, 1);
        assert_eq!(result.diagnostics[0].code, "E0419", "call: {call}");
    }
}

#[test]
fn message_path_calls_the_user_message_handler_hook() {
    // T8.10 (24.9.2.14): user_message_handler() is an empty extendable
    // hook on any_unit; the message path calls it once on the receiving
    // instance before the message text prints.
    let output = clean_output(
        "extend any_unit {\n    user_message_handler() is also { out(\"hook\\n\"); };\n};\n\
         extend sys {\n    run() is also { message(LOW, \"x\"); };\n};\n",
        1,
    );
    assert_eq!(output, "hook\nx\n");
}

#[test]
fn user_transaction_handler_hooks_are_extendable_names() {
    // T8.10: the transaction hooks parse as ordinary extendable methods
    // on any_unit; nothing calls them (there is no transaction
    // machinery), but an explicit call runs the extension.
    let output = clean_output(
        "extend any_unit {\n    \
         user_transaction_handler() is also { out(\"txn hook\\n\"); };\n};\n\
         extend sys {\n    run() is also { user_transaction_handler(); };\n};\n",
        1,
    );
    assert_eq!(output, "txn hook\n");
}

#[test]
fn for_each_over_an_enumerated_set_literal_iterates_the_enumerands() {
    // T8.11 (20.2.5): `for each (c) in [red..blue]` iterates the set's
    // items in order — the enumerands of the enumeral type. The 2019
    // clause generalizes the iteration to any item type (the plan's
    // C6); this enumeral case pins that reading.
    let output = clean_output(
        "type color: [red, green, blue];\n\
         extend sys {\n    run() is also {\n        \
         for each (c) in [red..blue] {\n            out(c, \" \");\n        };\n        \
         out(\"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "red green blue \n");
}

#[test]
fn for_each_over_an_integer_set_literal_and_a_set_value() {
    // T8.11: the same iteration over an integer set literal and over a
    // set value (set_of_values) — items in order.
    let output = clean_output(
        "extend sys {\n    run() is also {\n        \
         for each (n) in [2..4] {\n            out(n);\n        };\n        \
         out(\"\\n\");\n        \
         var s: set = set_of_values(color_t);\n        \
         for each (n) in s {\n            out(n);\n        };\n        \
         out(\"\\n\");\n    };\n};\n\
         type color_t: [a, b, c];\n",
        1,
    );
    assert_eq!(output, "234\n012\n");
}

#[test]
fn rerun_is_e0419() {
    // T8.1 (28.3.5.1): rerun() — deactivate at the end of the current
    // tick, reactivate next cycle — has no per-tick struct activation
    // in the coroutine model, so it is an honest E0419 stub.
    let result = run_source("extend sys {\n    run() is also { sys.rerun(); };\n};\n", 1);
    let diagnostic = &result.diagnostics[0];
    assert_eq!(diagnostic.code, "E0419");
    assert!(
        diagnostic
            .render()
            .contains("'rerun()' is recognised (clause 28.3.5.1)"),
        "rendered: {}",
        diagnostic.render()
    );
}

#[test]
fn swap_is_a_recognised_pack_tool_name() {
    // T8.12 (19.1): swap() parses as a builtin pack tool call; the
    // byte-order swap is not implemented, so the call reports E0419.
    let result = run_source("extend sys {\n    run() is also { swap(5); };\n};\n", 1);
    let diagnostic = &result.diagnostics[0];
    assert_eq!(diagnostic.code, "E0419");
    assert!(
        diagnostic
            .render()
            .contains("'swap()' is recognised (clause 19.1)"),
        "rendered: {}",
        diagnostic.render()
    );
}

#[test]
fn interface_typed_values_print_the_underlying_struct_or_null() {
    // T3.5: printing an interface-typed value renders the underlying
    // instance exactly as a struct-typed one does; a NULL interface
    // value (the default) prints NULL.
    let output = clean_output(
        "interface shape { area(): uint; };\n\
         struct square implementing shape { side: uint; area(): uint is { return side * side; }; };\n\
         extend sys {\n    run() is also {\n        var filled: shape = new square;\n        \
         out(filled, \"\\n\");\n        \
         var nothing: shape;\n        \
         out(nothing, \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "square{side: 0}\nNULL\n");
}

#[test]
fn interface_default_and_generated_fields_are_null() {
    // 5.2.9: an interface-typed field's default is NULL, and `gen` does
    // not invent an instance for it — the field stays NULL.
    let output = clean_output(
        "interface shape { area(): uint; };\n\
         struct holder {\n    sh: shape;\n};\n\
         extend sys {\n    run() is also {\n        var h: holder = new;\n        \
         gen h;\n        out(h.sh == NULL, \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "TRUE\n");
}

// The 2019 annotation API (IEEE 1647-2019 clause 35.4, task T10.3).
// The two-module shape: the annotation type is declared in one module,
// attached and queried from the next one (clause 35.2's same-module
// rule) — the same composed file the CLI's import driver builds
// (`parse_with_imports` in `main.rs`; the library surface has no import
// driver of its own).

/// [`run_source`] over the composed two-module file: `imported`'s
/// declarations splice in first, the per-file module-scope pass has run
/// inside each `parse_unchecked`, and the well-formedness pass runs
/// once over the merged set.
fn run_two_modules(imported: &str, source: &str, seed: u64) -> RunResult {
    let imported = e1647::parser::parse_unchecked(imported);
    let own = e1647::parser::parse_unchecked(source);
    let mut declarations = imported.declarations;
    declarations.extend(own.declarations);
    let mut diagnostics = imported.diagnostics;
    diagnostics.extend(own.diagnostics);
    diagnostics.extend(e1647::check::check_declarations(&declarations));
    assert!(
        diagnostics.is_empty(),
        "diagnostics: {:?}",
        diagnostics.iter().map(|d| d.render()).collect::<Vec<_>>()
    );
    let elaborated = elaborate(&declarations);
    assert!(
        elaborated.diagnostics.is_empty(),
        "elaboration diagnostics: {:?}",
        elaborated
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
    run(&elaborated, seed)
}

#[test]
fn annotation_api_get_all_annotations_lists_recorded_attachments() {
    // `@T.get_all_annotations()` (clause 35.4) returns the type's
    // recorded attachments as a list of source-like strings — the
    // documented representation of the annotation's fields in this
    // stage (attribute values rendered as written).
    let imported = "<'\nannotation @development { developer: string; date: string; release: uint; };\n\
                    repeatable annotation @reviewed { by: string; };\n'>";
    let source = "<'\nimport ann;\n\
                  @development(developer = \"jo\", release = 10) struct packet_s { x: uint; };\n\
                  @reviewed(by = \"ann\") @reviewed(by = \"bob\") struct checker_u { };\n\
                  extend sys {\n    run() is {\n        \
                  out(@development.get_all_annotations().size(), \"\\n\");\n        \
                  out(@development.get_all_annotations()[0], \"\\n\");\n        \
                  out(@reviewed.get_all_annotations().size(), \"\\n\");\n    \
                  };\n};\n'>";
    let result = run_two_modules(imported, source, 1);
    assert!(
        result.diagnostics.is_empty(),
        "run diagnostics: {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        result.output,
        "1\n@development(developer = \"jo\", release = 10)\n2\n"
    );
}

#[test]
fn annotation_api_element_surface_is_e0419_stubs() {
    // The element-taking calls and the definition-element surface are
    // recognised and arity-checked but report E0419 at run time: there
    // is no rf_definition_element model in this stage.
    let imported = "<'\nannotation @development { developer: string; };\n'>";
    let source = "<'\nimport ann;\n\
                  @development struct packet_s { x: uint; };\n\
                  extend sys {\n    run() is {\n        \
                  out(@development.get_attached_annotation(NULL));\n    \
                  };\n};\n'>";
    let result = run_two_modules(imported, source, 1);
    let rendered = result
        .diagnostics
        .iter()
        .map(|d| d.render())
        .collect::<Vec<_>>();
    assert_eq!(
        rendered,
        vec![
            "6:13: error[E0419]: 'get_attached_annotation()' is recognised (clause 35.4) but not implemented in this stage"
        ]
    );
}

#[test]
fn new_of_annotation_type_is_rejected() {
    // Clause 35.2: `new` is not allowed for an annotation type — the
    // annotation exists as recorded metadata, not as an instance.
    let imported = "<'\nannotation @development { developer: string; };\n'>";
    let source = "<'\nimport ann;\n\
                  @development struct packet_s { x: uint; };\n\
                  extend sys {\n    run() is {\n        \
                  out(new development);\n    \
                  };\n};\n'>";
    let result = run_two_modules(imported, source, 1);
    let rendered = result
        .diagnostics
        .iter()
        .map(|d| d.render())
        .collect::<Vec<_>>();
    assert_eq!(
        rendered,
        vec![
            "6:13: error[E0402]: cannot instantiate annotation type 'development' — 'new' is not allowed for annotations (clause 35.2)"
        ]
    );
}
// ---------------------------------------------------------------------------
// T9.1 (26.2): the `table … with { … }` preprocessor expansion.
// ---------------------------------------------------------------------------

/// The migration plan's T9.1 test: a struct member table declaring one
/// field per row, a set-literal table over actions and a `count` table.
fn table_regs_source() -> String {
    "<'\n\
     struct regs {\n    \
     table { #name | #width ;\n            \
     ctrl  | 8      ;\n            \
     stat  | 4      ;\n            \
     data  | 16     } with {\n        \
     <#name>: uint(bits: <#width>);\n        \
     keep <#name> < <#width>;\n    };\n};\n\
     extend sys {\n    \
     run() is also {\n        \
     var r: regs; gen r;\n        \
     table [1..3] with { out(\"row <#0> value <#1>\\n\"); };\n        \
     table count { a | 1; b | 2 } with { out(\"rows: <#1>\\n\"); };\n    };\n};\n'>\n"
        .to_owned()
}

#[test]
fn table_expansion_declares_members_and_iterates_rows() {
    // T9.1: the member table expands one field + constraint per row
    // (elab lists ctrl/stat/data at widths 8/4/16), the set-literal
    // table runs its body once per row with the row index and first
    // column substituted, and `count` reduces the data to the row
    // count.
    let parsed = e1647::parser::parse(&table_regs_source());
    assert!(
        parsed.diagnostics.is_empty(),
        "parse diagnostics: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
    let elaborated = elaborate(&parsed.declarations);
    assert!(
        elaborated.diagnostics.is_empty(),
        "elab diagnostics: {:?}",
        elaborated
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
    let regs = elaborated
        .types
        .iter()
        .find(|t| t.name == "regs")
        .expect("the regs type elaborated");
    let widths: Vec<(String, u32)> = regs
        .fields
        .iter()
        .filter_map(|field| match &field.ty {
            e1647::ast::TypeExpr::Scalar {
                width: Some(width), ..
            } => match width.as_ref() {
                e1647::ast::Expr::IntLiteral { text, .. } => text
                    .parse::<u32>()
                    .ok()
                    .map(|width| (field.name.clone(), width)),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(
        widths,
        vec![
            ("ctrl".to_owned(), 8),
            ("stat".to_owned(), 4),
            ("data".to_owned(), 16)
        ]
    );
    let result = run(&elaborated, 1);
    assert!(
        result.diagnostics.is_empty(),
        "run diagnostics: {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        result.output,
        "row 0 value 1\nrow 1 value 2\nrow 2 value 3\nrows: 2\n"
    );
}

#[test]
fn gen_chosen_from_picks_satisfying_elements_deterministically() {
    // T6.1 (10.5.8): `keep p chosen_from read_only(lps)` assigns p a
    // pointer to one of the read-only list's elements, chosen so the
    // other constraints on paths through the target hold (p.x > 5);
    // 20 generations print 20 TRUE lines, and the choice is uniform over
    // the satisfying elements under the seeded RNG (across fixed seeds
    // every satisfying value is reached).
    let source = "\n\
         struct packet_s { x: uint; };\n\
         struct holder {\n    !lps: list of packet_s;\n    p: packet_s;\n    \
         keep p chosen_from read_only(lps);\n    keep p.x > 5;\n};\n\
         extend sys {\n    run() is also {\n        var h: holder = new;\n        \
         for i from 0 to 9 { var q: packet_s = new; q.x = i; h.lps.add(q); };\n        \
         for k from 1 to 20 { gen h.p; out(h.p.x > 5, \" \", h.p.x, \"\\n\"); };\n    };\n};\n";
    let first = clean_output(source, 1);
    let lines: Vec<&str> = first.lines().collect();
    assert_eq!(lines.len(), 20);
    assert!(
        lines.iter().all(|line| line.starts_with("TRUE ")),
        "every generation must satisfy p.x > 5: {first:?}"
    );
    let second = clean_output(source, 1);
    assert_eq!(first, second, "the same seed must reproduce the run");
    let mut seen = std::collections::BTreeSet::new();
    for seed in 1..=5u64 {
        let output = clean_output(source, seed);
        for line in output.lines() {
            let value: i128 = line["TRUE ".len()..]
                .trim()
                .parse()
                .expect("integer output");
            assert!((6..=9).contains(&value), "constraint violated: x = {value}");
            seen.insert(value);
        }
    }
    assert_eq!(
        seen,
        std::collections::BTreeSet::from([6, 7, 8, 9]),
        "the uniform choice must reach every satisfying element across seeds"
    );
}

// --- Range-generated fields (clause 6.9.7, 10.3.13, 18.6, 29.13) ---

#[test]
fn rgf_reduced_set_satisfies_the_constraints_per_branch() {
    // The migration plan T5.2's worked example: `c => a < 10` and
    // `!c => b < 10` produce complementary reduced sets, and each
    // field's concrete value stays inside its set — 30 lines, all TRUE.
    let output = clean_output(
        "struct rg {\n\
         \x20   c: bool;\n\
         \x20   ~b: uint;\n\
         \x20   ~a: uint;\n\
         \x20   keep c => a < 10;\n\
         \x20   keep !c => b < 10;\n\
         };\n\
         extend sys {\n\
         \x20   run() is also {\n\
         \x20       var r: rg;\n\
         \x20       for i from 1 to 10 {\n\
         \x20           gen r;\n\
         \x20           if r.c then {\n\
         \x20               out(rgf_get_range(r.a) == [0..9]);\n\
         \x20               out(rgf_get_range(r.b) == [0..MAX_UINT]);\n\
         \x20           } else {\n\
         \x20               out(rgf_get_range(r.b) == [0..9]);\n\
         \x20               out(rgf_get_range(r.a) == [0..MAX_UINT]);\n\
         \x20           };\n\
         \x20           out(r.a in rgf_get_range(r.a) and r.b in rgf_get_range(r.b));\n\
         \x20       };\n\
         \x20   };\n\
         };\n",
        1,
    );
    assert_eq!(output, "TRUE".repeat(30));
}

#[test]
fn rgf_set_is_the_intersection_of_the_constraint_intervals() {
    // Supported shapes: `in` with a range, `!=` (complement within the
    // type range), a mirrored comparison, and a same-field `||` union.
    // The concrete value is the set's minimum.
    let output = clean_output(
        "struct bounded_s {\n\
         \x20   ~a: uint;\n\
         \x20   keep a in [2..6];\n\
         \x20   keep a != 5;\n\
         };\n\
         struct mirrored_s {\n\
         \x20   ~a: uint;\n\
         \x20   keep 10 > a;\n\
         };\n\
         struct union_s {\n\
         \x20   ~a: uint;\n\
         \x20   keep a == 3 || a == 7;\n\
         };\n\
         struct excluded_s {\n\
         \x20   ~a: uint;\n\
         \x20   keep a not in [0..4];\n\
         };\n\
         extend sys {\n\
         \x20   run() is also {\n\
         \x20       var bounded: bounded_s;\n\
         \x20       gen bounded;\n\
         \x20       out(bounded.a, \" \", rgf_get_range(bounded.a), \"\\n\");\n\
         \x20       var mirrored: mirrored_s;\n\
         \x20       gen mirrored;\n\
         \x20       out(mirrored.a, \" \", rgf_get_range(mirrored.a), \"\\n\");\n\
         \x20       var unioned: union_s;\n\
         \x20       gen unioned;\n\
         \x20       out(unioned.a, \" \", rgf_get_range(unioned.a), \"\\n\");\n\
         \x20       var excluded: excluded_s;\n\
         \x20       gen excluded;\n\
         \x20       out(excluded.a, \" \", rgf_get_range(excluded.a), \"\\n\");\n\
         \x20   };\n\
         };\n",
        1,
    );
    assert_eq!(
        output,
        "2 [2..4,6]\n\
         0 [0..9]\n\
         3 [3,7]\n\
         5 [5..4294967295]\n"
    );
}

#[test]
fn nested_table_refers_to_the_enclosing_row() {
    // T9.1's nested test: the outer table iterates { A; B }, the inner
    // table runs inside each outer row; `<##1>` reads the enclosing
    // row's first cell, `<#1>` the inner row's.
    let output = clean_output(
        "extend sys {\n    \
         run() is also {\n        \
         table { A; B } with {\n            \
         table [1..2] with { out(\"<##1>-<#1>\\n\"); };\n        \
         };\n    };\n};\n",
        1,
    );
    assert_eq!(output, "A-1\nA-2\nB-1\nB-2\n");
}

#[test]
fn count_excludes_the_header_row() {
    // With `count`, the header row is not a data row: the two data
    // rows count as 2 (a headerless two-row table counts 2 as well —
    // the plan's `table count { a | 1; b | 2 }` case covers that).
    let output = clean_output(
        "extend sys {\n    \
         run() is also {\n        \
         table count { #a | #b ; 1 | 2 ; 3 | 4 } with { out(\"count=<#1>\\n\"); };\n    };\n};\n",
        1,
    );
    assert_eq!(output, "count=2\n");
}

// ---------------------------------------------------------------------------
// T9.2 (26.3): `table from operator(...) using filter/unique`.
// ---------------------------------------------------------------------------

#[test]
fn table_from_csv_to_table_with_filter() {
    // T9.2's test: the rows come from tests/data/regs.csv's `regs`
    // table; the filter drops the `dummy` row, so `ctrl:8` and
    // `stat:4` print.
    let path = format!(
        "{}/tests/data/regs",
        env!("CARGO_MANIFEST_DIR").replace('\\', "/")
    );
    let source = format!(
        "extend sys {{\n    \
         run() is also {{\n        \
         table from csv_to_table(\"{path}\", \"regs\")\n            \
         using filter = table_column(\"name\") != \"dummy\"\n            \
         with {{ out(\"<#name>:<#width>\\n\"); }};\n    }};\n}};\n"
    );
    let output = clean_output(&source, 1);
    assert_eq!(output, "ctrl:8\nstat:4\n");
}

#[test]
fn table_from_csv_to_table_with_unique() {
    // The unique option keeps the first row per distinct value,
    // applied after the filter — over the `dups` table whose widths
    // repeat (8, 4, 8, 4, 8), only the first 8 and the first 4 stay.
    let path = format!(
        "{}/tests/data/regs",
        env!("CARGO_MANIFEST_DIR").replace('\\', "/")
    );
    let source = format!(
        "extend sys {{\n    \
         run() is also {{\n        \
         table from csv_to_table(\"{path}\", \"dups\")\n            \
         using unique = table_column(\"width\")\n            \
         with {{ out(\"<#name>\\n\"); }};\n    }};\n}};\n"
    );
    let output = clean_output(&source, 1);
    assert_eq!(output, "a\nb\n");
}

// ---------------------------------------------------------------------------
// T9.3 (32.3): the procedural table API.
// ---------------------------------------------------------------------------

#[test]
fn table_procedural_api_basics() {
    // T9.3's test: create_table + add_row + set_data/read round-trip;
    // rows number from 0, columns from 1.
    let output = clean_output(
        "extend sys {\n    \
         run() is also {\n        \
         var t := table_manager.create_table({\"a\";\"b\"});\n        \
         t.add_row();\n        \
         t.get_row(0).set_data(1, \"x\");\n        \
         out(t.get_rows_num(), \" \", t.get_columns_num(), \" \", \
         t.get_row(0).get_data(1), \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "1 2 x\n");
}

#[test]
fn table_row_index_column_and_row_api() {
    // Column 0 is the row index; the row/row_with_data creators and
    // the source-info copies work on standalone rows.
    let output = clean_output(
        "extend sys {\n    \
         run() is also {\n        \
         var t := table_manager.create_table({\"a\";\"b\"});\n        \
         t.add_row(); t.add_row();\n        \
         out(t.get_row(1).get_data(0), \"\\n\");\n        \
         var r := table_manager.create_row_with_data({\"p\";\"q\"});\n        \
         out(r.get_size(), \" \", r.get_data(1), r.get_data(2), \"\\n\");\n        \
         var s := table_manager.create_row(2);\n        \
         s.copy_source_info_from(r);\n        \
         s.set_data(2, \"z\");\n        \
         out(s.get_data(2), \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "1\n2 pq\nz\n");
}

#[test]
fn table_read_from_csv_is_cached_and_read_only() {
    // A table read from a file is read-only: set_data through a row is
    // the copy-first runtime error; the copy is writable.
    let parsed = e1647::parser::parse(&format!(
        "extend sys {{\n    \
         run() is also {{\n        \
         var t := table_manager.read_table_from_csv(\"{}/tests/data/regs\", \"regs\");\n        \
         t.get_row(0).set_data(1, \"9\");\n    }};\n}};\n",
        env!("CARGO_MANIFEST_DIR").replace('\\', "/")
    ));
    assert!(parsed.diagnostics.is_empty(), "parse failed");
    let elaborated = elaborate(&parsed.declarations);
    assert!(elaborated.diagnostics.is_empty(), "elab failed");
    let result = run(&elaborated, 1);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.render().contains("read-only")),
        "expected the copy-first read-only error, got {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
}

#[test]
fn gen_chosen_from_target_is_an_alias_of_one_list_element() {
    // T6.1: pointer semantics — the target holds one of the list's
    // elements (object identity), not a copy. `has(it list_ptr== ...)`
    // is not in the runtime subset, so the alias is found by comparing
    // object equality against each element.
    let output = clean_output(
        "\n\
         struct packet_s { x: uint; };\n\
         struct holder {\n    !lps: list of packet_s;\n    p: packet_s;\n    \
         keep p chosen_from read_only(lps);\n};\n\
         extend sys {\n    run() is also {\n        var h: holder = new;\n        \
         for i from 0 to 9 { var q: packet_s = new; q.x = i + 3; h.lps.add(q); };\n        \
         gen h.p;\n        var hit: int = -1;\n        \
         for i from 0 to 9 { if h.p == h.lps[i] { hit = i; }; };\n        \
         out(hit >= 0, \"\\n\");\n        \
         out(h.p.x == h.lps[hit].x, \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "TRUE\nTRUE\n");
}

#[test]
fn gen_whole_object_applies_chosen_from_to_the_target_field() {
    // T6.1: `gen h` (the whole enclosing instance) also applies the
    // chosen_from constraint to its target field instead of sampling a
    // fresh packet, and the other constraints on paths through the
    // target hold.
    let output = clean_output(
        "\n\
         struct packet_s { x: uint; };\n\
         struct holder {\n    !lps: list of packet_s;\n    p: packet_s;\n    \
         keep p chosen_from read_only(lps);\n    keep p.x > 5;\n};\n\
         extend sys {\n    run() is also {\n        var h: holder = new;\n        \
         for i from 0 to 9 { var q: packet_s = new; q.x = i; h.lps.add(q); };\n        \
         gen h;\n        out(h.p.x > 5, \"\\n\");\n        \
         var hit: int = -1;\n        \
         for i from 0 to 9 { if h.p == h.lps[i] { hit = i; }; };\n        \
         out(hit >= 0, \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(output, "TRUE\nTRUE\n");
}

#[test]
fn gen_chosen_from_no_satisfying_element_reports_e0417() {
    // T6.1: after every element's x drops to 1, no element satisfies
    // p.x > 5 — the generation contradiction is E0417.
    let result = run_source(
        "\n\
         struct packet_s { x: uint; };\n\
         struct holder {\n    !lps: list of packet_s;\n    p: packet_s;\n    \
         keep p chosen_from read_only(lps);\n    keep p.x > 5;\n};\n\
         extend sys {\n    run() is also {\n        var h: holder = new;\n        \
         for i from 0 to 9 { var q: packet_s = new; q.x = i; h.lps.add(q); };\n        \
         for each (e) in h.lps { e.x = 1; };\n        \
         gen h.p;\n    };\n};\n",
        1,
    );
    let diagnostic = &result.diagnostics[0];
    assert_eq!(diagnostic.code, "E0417");
    assert!(
        diagnostic
            .render()
            .contains("chosen_from: no element of 'lps' satisfies the constraints on 'p'"),
        "rendered: {}",
        diagnostic.render()
    );
}

#[test]
fn table_copy_is_writable_and_read_helpers_work() {
    // copy() lifts the read-only flag; the csv readers report their
    // source, and the manager's line helpers answer per 32.3.3.
    let path = format!(
        "{}/tests/data/regs",
        env!("CARGO_MANIFEST_DIR").replace('\\', "/")
    );
    let output = clean_output(
        &format!(
            "extend sys {{\n    \
             run() is also {{\n        \
             var t := table_manager.read_table_from_csv(\"{path}\", \"regs\");\n        \
             var c := t.copy();\n        \
             c.get_row(0).set_data(1, \"9\");\n        \
             out(c.get_row(0).get_data(1), \"\\n\");\n        \
             out(t.get_row(0).get_csv_file_name() != \"\", \"\\n\");\n        \
             out(t.get_row(0).get_csv_line_num(), \"\\n\");\n        \
             out(table_manager.is_empty_line(\"  , \"), \"\\n\");\n        \
             out(table_manager.get_table_tag_from_row(\"regs,x\"), \"\\n\");\n    }};\n}};\n"
        ),
        1,
    );
    assert_eq!(output, "9\nTRUE\n3\nTRUE\nregs\n");
}

#[test]
fn excel_table_readers_report_the_stub() {
    // The excel readers are the honest 32.3 stubs: recognised, E0419
    // at run time.
    let parsed = e1647::parser::parse(
        "extend sys {\n    \
         run() is also {\n        \
         table_manager.read_table_from_excel(\"f\", \"s\", \"t\");\n    };\n};\n",
    );
    assert!(parsed.diagnostics.is_empty(), "parse failed");
    let elaborated = elaborate(&parsed.declarations);
    assert!(elaborated.diagnostics.is_empty(), "elab failed");
    let result = run(&elaborated, 1);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.render().contains("E0419")),
        "expected the E0419 stub, got {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
}

#[test]
fn gen_chosen_from_empty_list_reports_e0417() {
    // T6.1: an empty read-only list is a generation contradiction too.
    let result = run_source(
        "\n\
         struct packet_s { x: uint; };\n\
         struct holder {\n    !lps: list of packet_s;\n    p: packet_s;\n    \
         keep p chosen_from read_only(lps);\n};\n\
         extend sys {\n    run() is also {\n        var h: holder = new;\n        \
         gen h.p;\n    };\n};\n",
        1,
    );
    let diagnostic = &result.diagnostics[0];
    assert_eq!(diagnostic.code, "E0417");
    assert!(
        diagnostic
            .render()
            .contains("chosen_from: no element of 'lps' satisfies the constraints on 'p'"),
        "rendered: {}",
        diagnostic.render()
    );
}

#[test]
fn gen_chosen_from_pointer_equality_constraint_is_e0402() {
    // T6.1: a pointer-equality constraint on the chosen_from target is
    // an unsupported shape, reported instead of silently ignored.
    let result = run_source(
        "\n\
         struct packet_s { x: uint; };\n\
         struct holder {\n    lps: list of packet_s;\n    p: packet_s;\n    q: packet_s;\n    \
         keep p chosen_from read_only(lps);\n    keep p == q;\n};\n\
         extend sys {\n    run() is also {\n        var h: holder = new;\n        \
         var q: packet_s = new; h.lps.add(q);\n        \
         gen h.p;\n    };\n};\n",
        1,
    );
    let diagnostic = &result.diagnostics[0];
    assert_eq!(diagnostic.code, "E0402");
    assert!(
        diagnostic
            .render()
            .contains("pointer-equality constraint on the target 'p' is not supported"),
        "rendered: {}",
        diagnostic.render()
    );
}

#[test]
fn gen_chosen_from_nongenerative_target_is_e0402() {
    // T6.1: the target must be a generative struct field — a `!` field
    // keeps its value and is never assigned by gen.
    let result = run_source(
        "\n\
         struct packet_s { x: uint; };\n\
         struct holder {\n    !lps: list of packet_s;\n    !p: packet_s;\n    \
         keep p chosen_from read_only(lps);\n};\n\
         extend sys {\n    run() is also {\n        var h: holder = new;\n        \
         var q: packet_s = new; h.lps.add(q);\n        \
         gen h.p;\n    };\n};\n",
        1,
    );
    let diagnostic = &result.diagnostics[0];
    assert_eq!(diagnostic.code, "E0402");
    assert!(
        diagnostic
            .render()
            .contains("chosen_from target 'p' must be a generative struct field"),
        "rendered: {}",
        diagnostic.render()
    );
}

#[test]
fn rgf_was_reduced_flags_whether_constraints_narrowed_the_type_range() {
    // T5.4: reduced TRUE where the constraints narrowed the set, FALSE
    // for a field whose set is the type's full range.
    let output = clean_output(
        "struct pair_s {\n\
         \x20   c: bool;\n\
         \x20   ~a: uint;\n\
         \x20   ~b: uint;\n\
         \x20   keep c == TRUE;\n\
         \x20   keep a < 10;\n\
         };\n\
         extend sys {\n\
         \x20   run() is also {\n\
         \x20       var r: pair_s;\n\
         \x20       gen r;\n\
         \x20       out(rgf_was_reduced(r.a), \" \", rgf_was_reduced(r.b), \"\\n\");\n\
         \x20   };\n\
         };\n",
        1,
    );
    assert_eq!(output, "TRUE FALSE\n");
}

#[test]
fn rgf_assign_within_and_outside_the_generated_set() {
    // T5.3: `rgf=` assigns a value of the generated set; outside it the
    // run reports E0415 with the set rendered as written. No automatic
    // width cast — the membership test is the whole check.
    let source = "struct sized_s {\n\
                  \x20   c: bool;\n\
                  \x20   ~a: uint;\n\
                  \x20   keep c == TRUE;\n\
                  \x20   keep a < 10;\n\
                  };\n\
                  extend sys {\n\
                  \x20   run() is also {\n\
                  \x20       var r: sized_s;\n\
                  \x20       gen r;\n\
                  \x20       r.a rgf= 7;\n\
                  \x20       out(r.a);\n\
                  \x20       r.a rgf= 12;\n\
                  \x20       out(r.a);\n\
                  \x20   };\n\
                  };\n";
    let result = run_source(source, 1);
    assert_eq!(result.output, "7");
    let diagnostic = &result.diagnostics[0];
    assert_eq!(diagnostic.code, "E0415");
    assert!(
        diagnostic
            .message
            .contains("value 12 is outside the generated range [0..9] of 'a'"),
        "{}",
        diagnostic.message
    );
}

#[test]
fn rgf_queries_before_generation() {
    // T5.4: `rgf_was_generated` is FALSE before any `gen`; the range
    // queries report E0416 until the field was generated.
    let source = "struct lone_s {\n\
                  \x20   ~a: uint;\n\
                  };\n\
                  extend sys {\n\
                  \x20   run() is also {\n\
                  \x20       var r: lone_s;\n\
                  \x20       out(rgf_was_generated(r.a));\n\
                  \x20       out(rgf_get_range(r.a));\n\
                  \x20   };\n\
                  };\n";
    let result = run_source(source, 1);
    assert_eq!(result.output, "FALSE");
    let codes: Vec<&str> = result
        .diagnostics
        .iter()
        .map(|d| d.code)
        .collect::<Vec<_>>();
    assert_eq!(codes, vec!["E0416"]);
    assert!(
        result.diagnostics[0]
            .message
            .contains("rgf query/assign before generation"),
        "{}",
        result.diagnostics[0].message
    );
}

#[test]
fn rgf_unsupported_constraint_shapes_report_e0317() {
    // T5.2: arithmetic over the rgf field, a comparison between two rgf
    // fields, a negation over an rgf atom, and two atoms naming
    // different fields are outside the supported shapes.
    let shapes = [
        ("arith_s", "keep a + 1 < 5;"),
        ("coupled_s", "keep a < b;"),
        ("negated_s", "keep !(a > 3);"),
        ("two_field_s", "keep a > 3 && b < 10;"),
    ];
    for (name, constraint) in shapes {
        let source = format!(
            "struct {name} {{\n\
             \x20   ~a: uint;\n\
             \x20   ~b: uint;\n\
             \x20   {constraint}\n\
             }};\n\
             extend sys {{\n\
             \x20   run() is also {{\n\
             \x20       var r: {name};\n\
             \x20       gen r;\n\
             \x20   }};\n\
             }};\n"
        );
        let result = run_source(&source, 1);
        let codes: Vec<&str> = result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>();
        assert_eq!(codes, vec!["E0317"], "{name}: {source}");
        assert!(
            result.diagnostics[0]
                .message
                .contains("unsupported constraint shape for range-generated field"),
            "{name}: {}",
            result.diagnostics[0].message
        );
    }
}

#[test]
fn rgf_implication_with_false_antecedent_keeps_the_full_range() {
    // The implication rule: a FALSE antecedent contributes the field
    // type's full range, so the set is unreduced and the minimum is the
    // type's bottom.
    let output = clean_output(
        "struct branch_s {\n\
         \x20   c: bool;\n\
         \x20   ~a: uint;\n\
         \x20   ~b: uint;\n\
         \x20   keep c == FALSE;\n\
         \x20   keep c => a < 3;\n\
         \x20   keep !c => b < 4;\n\
         };\n\
         extend sys {\n\
         \x20   run() is also {\n\
         \x20       var r: branch_s;\n\
         \x20       gen r;\n\
         \x20       out(r.a, \" \", rgf_get_range(r.a) == [0..MAX_UINT], \" \");\n\
         \x20       out(r.b, \" \", rgf_get_range(r.b) == [0..3], \"\\n\");\n\
         \x20   };\n\
         };\n",
        1,
    );
    assert_eq!(output, "0 TRUE 0 TRUE\n");
}

#[test]
fn rgf_enum_and_bool_fields_carry_index_sets() {
    // The value model keeps integer intervals, so an enumeral field's
    // generated set is its enumerand indices (the set model's numbering)
    // and a bool field's is {0, 1}; the concrete value maps back to the
    // enumerand or the boolean.
    let output = clean_output(
        "type color_e: [RED, GREEN, BLUE];\n\
         struct colored_s {\n\
         \x20   ~c: color_e;\n\
         \x20   keep c != GREEN;\n\
         };\n\
         struct flagged_s {\n\
         \x20   ~f: bool;\n\
         \x20   keep f == TRUE;\n\
         };\n\
         extend sys {\n\
         \x20   run() is also {\n\
         \x20       var colored: colored_s;\n\
         \x20       gen colored;\n\
         \x20       out(colored.c, \" \", rgf_get_range(colored.c), \"\\n\");\n\
         \x20       var flagged: flagged_s;\n\
         \x20       gen flagged;\n\
         \x20       out(flagged.f, \" \", rgf_get_range(flagged.f), \"\\n\");\n\
         \x20   };\n\
         };\n",
        1,
    );
    assert_eq!(output, "RED [0,2]\nTRUE [1]\n");
}

#[test]
fn in_accepts_a_general_set_or_list_expression() {
    // The `in` right operand is a set expression (4.11.6's set-exp):
    // a set value is tested by interval membership, a list by item
    // equality.
    let output = clean_output(
        "extend sys {\n\
         \x20   run() is also {\n\
         \x20       var l: list of uint = {1; 2; 3};\n\
         \x20       out(2 in l, \" \", 9 in l, \" \");\n\
         \x20       var s: set = [1..5];\n\
         \x20       out(4 in s, \" \", 9 in s, \"\\n\");\n\
         \x20   };\n\
         };\n",
        1,
    );
    assert_eq!(output, "TRUE FALSE TRUE FALSE\n");
}

#[test]
fn template_value_parameter_substitutes_in_method_bodies() {
    // 8.5.2 (the migration's T4.1): a value parameter's constant
    // substitutes where a constant expression of that type is expected
    // — a method body's return, a condition, a call argument — and the
    // defaults-omitted spelling instantiates the same instance as the
    // explicit one.
    let output = clean_output(
        "template struct fifo of (<t'type>, <depth'exp>: uint = 4) {\n    \
         !items: list of <t'type>;\n    \
         capacity(): uint is { return <depth'exp>; };\n    \
         push(x: <t'type>): bool is {\n        \
         if items.size() >= <depth'exp> then { return FALSE };\n        \
         items.add(x); return TRUE;\n    };\n};\n\
         extend sys {\n    run() is also {\n        \
         var f8: fifo of (byte, 8) = new;\n        \
         var f4: fifo of (int) = new;\n        \
         out(f8.capacity()); out(f4.capacity());\n        \
         for i from 1 to 5 { compute f4.push(i) };\n        \
         out(f4.items.size());\n    };\n};\n",
        1,
    );
    // capacity 8 and 4; the fifth push of 5 against capacity 4 is
    // rejected, so the list holds 4.
    assert_eq!(output, "844");
}

#[test]
fn template_extend_members_apply_by_bounding_criteria() {
    // 8.6 (the migration's T4.4): an extension's members apply to the
    // instances whose actuals match the entries and satisfy the
    // conditions — `wide()` exists only on the 8 instance, `narrow()`
    // only on the 4 one.
    let output = clean_output(
        "type color_t: [red, green];\n\
         template struct packet of (<k'type>, <w'exp>: uint) { !kind: <k'type>; };\n\
         template extend packet of (<e'type>: enum, <n'exp>: uint) conditions { <n'exp> >= 8 } {\n    \
         wide(): bool is { return TRUE; };\n};\n\
         template extend packet of (color_t, 4) {\n    \
         narrow(): bool is { return TRUE; };\n};\n\
         extend sys {\n    run() is also {\n        \
         var a: packet of (color_t, 8) = new;\n        \
         var b: packet of (color_t, 4) = new;\n        \
         out(a.wide()); out(b.narrow());\n    };\n};\n",
        1,
    );
    assert_eq!(output, "TRUETRUE");
}

// ---------------------------------------------------------------------------
// Phase 11 — the reflection catalogue (T11.1) and the clause 32.2
// instance iterator (T11.2). Catalogue: docs/REFLECTION_2019.md.
// ---------------------------------------------------------------------------

/// The plan's T11.2 test tree: `sys.a`, `sys.a.b`, `sys.lst[2]`.
const INSTANCE_TREE: &str = "unit leaf_u {\n    \
     x: uint;\n};\n\
     unit mid_u {\n    \
     b: leaf_u;\n};\n\
     extend sys {\n    \
     a: mid_u;\n    \
     lst: list of leaf_u;\n    \
     keep lst.size() == 2;\n};\n";

#[test]
fn instance_iterator_walks_the_field_tree_in_field_order() {
    // 32.2 (T11.2): the depth-first walk visits the root first (its
    // path string is `sys`), then each instance field's own subtree in
    // declaration order, list items flattened in index order — the
    // plan's expected `get_current_path_string()` sequence.
    let output = clean_output(
        &format!(
            "{}extend sys {{\n    run() is also {{\n        \
             var it: instance_iterator = new instance_iterator;\n        \
             it.start(sys);\n        \
             out(it.get_current_path_string(), \"\\n\");\n        \
             while it.next() {{\n            \
             out(it.get_current_path_string(), \"\\n\");\n        \
             }};\n    }};\n}};\n",
            INSTANCE_TREE
        ),
        1,
    );
    assert_eq!(output, "sys\nsys.a\nsys.a.b\nsys.lst[0]\nsys.lst[1]\n");
}

#[test]
fn instance_iterator_start_all_reports_depth_and_path_items() {
    // 32.2: `start_all()` roots at sys without an explicit root; the
    // depth is the field-step count from the iteration root (the root
    // itself is 0, a list item shares its field's depth); a path item
    // reports its field, its index chain and its instance, and the
    // root item has none of them (NULL).
    let output = clean_output(
        &format!(
            "{}extend sys {{\n    run() is also {{\n        \
             var it: instance_iterator = new instance_iterator;\n        \
             it.start_all();\n        \
             var root_item: instance_path_item = it.get_path_item_by_index(0);\n        \
             out(root_item.get_list_indices().size(), \"\\n\");\n        \
             out(it.get_current_depth(), \" \", it.get_current_path_item().get_field(), \"\\n\");\n        \
             while it.next() {{\n            \
             var item: instance_path_item = it.get_current_path_item();\n            \
             out(it.get_current_depth(), \" \", item.get_field(), \" \", \
             item.get_list_index(), \" \", item.get_list_indices().size(), \"\\n\");\n        \
             }};\n    }};\n}};\n",
            INSTANCE_TREE
        ),
        1,
    );
    assert_eq!(
        output,
        "0\n0 NULL\n1 a NULL 0\n2 b NULL 0\n1 lst 0 1\n1 lst 1 1\n"
    );
}

#[test]
fn instance_iterator_get_current_instance_reads_the_visited_object() {
    // 32.2: `get_current_instance()` is the visited object itself — a
    // leaf's field reads through it.
    let output = clean_output(
        &format!(
            "{}extend sys {{\n    run() is also {{\n        \
             var it: instance_iterator = new instance_iterator;\n        \
             it.start(sys.a);\n        \
             it.next();\n        \
             out(it.get_current_instance().x == sys.a.b.x, \"\\n\");\n    }};\n}};\n",
            INSTANCE_TREE
        ),
        1,
    );
    assert_eq!(output, "TRUE\n");
}

#[test]
fn instance_iterator_does_not_revisit_a_shared_object() {
    // 32.2: an object already visited is not revisited — the leaf
    // reached as `sys.a.b` and appended to `sys.lst` afterwards is
    // walked once: `sys.lst[2]` never appears, the list slots the
    // `gen` produced are distinct objects and are visited.
    let output = clean_output(
        &format!(
            "{}extend sys {{\n    run() is also {{\n        \
             sys.lst.add(sys.a.b);\n        \
             var it: instance_iterator = new instance_iterator;\n        \
             it.start(sys);\n        \
             while it.next() {{\n            \
             out(it.get_current_path_string(), \"\\n\");\n        \
             }};\n    }};\n}};\n",
            INSTANCE_TREE
        ),
        1,
    );
    assert_eq!(output, "sys.a\nsys.a.b\nsys.lst[0]\nsys.lst[1]\n");
}

#[test]
fn instance_iterator_next_finish_and_stopped_states() {
    // 32.2: `next()` runs FALSE past the end; the current-instance
    // queries then report honestly, as they do before a start and for
    // an out-of-range path index; `finish()` ends the iteration.
    let result = run_source(
        "extend sys {\n    run() is also {\n        \
         var it: instance_iterator = new instance_iterator;\n        \
         out(it.get_current_path_string(), \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(result.diagnostics[0].code, "E0402");
    assert!(
        result.diagnostics[0]
            .message
            .contains("has not been started"),
        "not-started diagnostic: {:?}",
        result.diagnostics[0].message
    );

    let result = run_source(
        "extend sys {\n    run() is also {\n        \
         var it: instance_iterator = new instance_iterator;\n        \
         it.start_all();\n        \
         while it.next() {};\n        \
         out(it.get_current_instance(), \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(result.diagnostics[0].code, "E0402");
    assert!(
        result.diagnostics[0].message.contains("walk is over"),
        "finished diagnostic: {:?}",
        result.diagnostics[0].message
    );

    let result = run_source(
        "extend sys {\n    run() is also {\n        \
         var it: instance_iterator = new instance_iterator;\n        \
         it.start_all();\n        \
         out(it.get_path_item_by_index(9), \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(result.diagnostics[0].code, "E0404");
    assert!(
        result.diagnostics[0].message.contains("path item index 9"),
        "range diagnostic: {:?}",
        result.diagnostics[0].message
    );

    let result = run_source(
        "extend sys {\n    run() is also {\n        \
         var it: instance_iterator = new instance_iterator;\n        \
         it.start_all();\n        \
         it.finish();\n        \
         out(it.get_current_path_string(), \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(result.diagnostics[0].code, "E0402");
    assert!(
        result.diagnostics[0]
            .message
            .contains("has not been started"),
        "after-finish diagnostic: {:?}",
        result.diagnostics[0].message
    );
}

#[test]
fn instance_iterator_deep_copy_is_independent() {
    // 29.2.2 over the 32.2 iterator: a copy records the same walk but
    // moves its own cursor.
    let output = clean_output(
        &format!(
            "{}extend sys {{\n    run() is also {{\n        \
             var it: instance_iterator = new instance_iterator;\n        \
             it.start(sys);\n        \
             var twin: instance_iterator = deep_copy(it);\n        \
             out(it.next(), \" \", twin.get_current_path_string(), \"\\n\");\n        \
             out(twin.next(), \" \", it.get_current_path_string(), \"\\n\");\n    }};\n}};\n",
            INSTANCE_TREE
        ),
        1,
    );
    assert_eq!(output, "TRUE sys\nTRUE sys.a\n");
}

#[test]
fn reflection_stub_calls_report_e0419_with_their_clause() {
    // T11.1: a recognised catalogue name resolves to the honest E0419
    // — the clause in the message, never "unknown name" — in the
    // member form, the bare rf_manager-query form and the construction
    // forms; the clause 32.2 hooks are stubs too.
    let cases: &[(&str, &str)] = &[
        (
            "sys.get_check_actions();",
            "'get_check_actions()' is recognised (clause 32.1.2.5.13) but not implemented in this stage",
        ),
        (
            "get_user_template_types();",
            "'get_user_template_types()' is recognised (clause 32.1.2.17) but not implemented in this stage",
        ),
        (
            "eval_exp_string(\"1\");",
            "'eval_exp_string()' is recognised (clause 32.1.4) but not implemented in this stage",
        ),
        (
            "delete_list_element(NULL, 0);",
            "'delete_list_element()' is recognised (clause 32.1.4) but not implemented in this stage",
        ),
    ];
    for (call, message) in cases {
        let result = run_source(
            &format!("extend sys {{\n    run() is also {{\n        {call}\n    }};\n}};\n"),
            1,
        );
        assert_eq!(result.diagnostics.len(), 1, "call: {call}");
        assert_eq!(result.diagnostics[0].code, "E0419", "call: {call}");
        assert_eq!(result.diagnostics[0].message, *message, "call: {call}");
    }
    for construction in ["new unit_instance_iterator", "new instance_path_item"] {
        let result = run_source(
            &format!(
                "extend sys {{\n    run() is also {{\n        \
                 var v: unit_instance_iterator = {construction};\n    }};\n}};\n"
            ),
            1,
        );
        assert_eq!(result.diagnostics.len(), 1, "construction: {construction}");
        assert_eq!(result.diagnostics[0].code, "E0419");
        assert!(
            result.diagnostics[0].message.starts_with(&format!(
                "'{}' is recognised (clause 32.2)",
                construction.trim_start_matches("new ")
            )),
            "construction diagnostic: {:?}",
            result.diagnostics[0].message
        );
    }
    let result = run_source(
        "extend sys {\n    run() is also {\n        \
         var it: instance_iterator = new instance_iterator;\n        \
         it.start(sys);\n        \
         out(it.skip_instance(), \"\\n\");\n    };\n};\n",
        1,
    );
    assert_eq!(
        result.diagnostics[0].render(),
        "5:13: error[E0419]: 'skip_instance()' is recognised (clause 32.2) but not implemented in this stage"
    );
}

#[test]
fn reflection_stub_yields_to_a_user_method_of_the_same_name() {
    // The stub table is consulted only when nothing else resolves: a
    // user-defined `get_range()` wins over the 32.1.2.5.4 stub.
    let output = clean_output(
        "extend sys {\n    run() is also {\n        \
         out(get_range(), \"\\n\");\n    };\n};\n\
     extend sys {\n    get_range(): string is { return \"user\"; };\n};\n",
        1,
    );
    assert_eq!(output, "user\n");
}
// ---------------------------------------------------------------------------
// Phase 7 (T7.1–T7.4): custom numeric types (5.2.15, 5.8.4, clause 31),
// the operator dispatch, and the 5.6.3 precision rules.
// ---------------------------------------------------------------------------

/// The fixed-point struct the plan's T7.2 runtime test declares: `raw`
/// stores the value scaled by 100.
const FIXED_POINT_STRUCT: &str = "\
struct fx_s implementing numeric_i of (fx_s, TRUE, 32) {\n\
    !raw: int;\n\
    set_initial_value() is { raw = 0; };\n\
    numeric_assign(other: fx_s) is { raw = other.raw; };\n\
    numeric_from_int(v: int) is { raw = v * 100; };\n\
    numeric_to_int(): int is { return raw / 100; };\n\
    numeric_add(other: fx_s, res: fx_s) is { res.raw = raw + other.raw; };\n\
    numeric_compare(other: fx_s): int is {\n\
        if raw < other.raw then { return -1 } else if raw > other.raw then { return 1 } else { return 0 };\n\
    };\n\
    numeric_to_string(radix: numeric_format_kind): string is { return append(raw / 100, \".\", raw % 100); };\n\
};\n\
numeric_type fx_t: fx_s;\n";

#[test]
fn custom_numeric_fixed_point_operators_and_missing_method() {
    // The plan's T7.2 runtime test (5.8.4, clause 31): `a + b`
    // dispatches to numeric_add with a fresh result instance, `out`
    // renders through numeric_to_string(dec), `as_a(int)` through
    // numeric_to_int, `<` through numeric_compare — and `a * b`
    // without a numeric_multiply definition is the runtime E0418.
    let result = run_source(
        &format!(
            "{FIXED_POINT_STRUCT}extend sys {{\n    \
             run() is also {{\n        \
             var a: fx_t = 3;\n        \
             var b: fx_t = 4;\n        \
             var c: fx_t = a + b;\n        \
             out(c);\n        \
             out(c.as_a(int));\n        \
             out(a < b);\n        \
             var d: fx_t = a * b;\n    \
             }};\n}};\n"
        ),
        1,
    );
    assert_eq!(result.output, "7.07TRUE");
    assert_eq!(result.diagnostics.len(), 1);
    let rendered = result.diagnostics[0].render();
    assert!(
        rendered.contains("error[E0418]")
            && rendered.contains("operation '*' is not implemented by numeric type 'fx_t'")
            && rendered.contains("(numeric_multiply missing)"),
        "unexpected diagnostic: {rendered}"
    );
}

#[test]
fn custom_numeric_value_semantics_and_conversions() {
    // T7.1's value model: binding a custom numeric value into any slot
    // copies it (`numeric_assign`), a plain scalar builds into the
    // type through `numeric_from_int` (T7.3's implicit conversions),
    // and comparisons dispatch through `numeric_compare`.
    let output = clean_output(
        &format!(
            "{FIXED_POINT_STRUCT}extend sys {{\n    \
             run() is also {{\n        \
             var a: fx_t = 3;\n        \
             var e: fx_t = a;\n        \
             out(a == e);\n        \
             var b: fx_t = 4;\n        \
             out(b > a);\n        \
             var c: fx_t = a + b;\n        \
             out(c);\n        \
             var f: fx_t = a + a;\n        \
             out(f.as_a(int));\n        \
             var n: int = 42;\n        \
             var g: fx_t = n;\n        \
             out(g);\n        \
             var h := a;\n        \
             out(h == a);\n    \
             }};\n}};\n"
        ),
        1,
    );
    assert_eq!(output, "TRUETRUE7.0642.0TRUE");
}

#[test]
fn custom_numeric_mixed_scalar_operand_converts() {
    // T7.3: arithmetic on a custom numeric and a plain scalar converts
    // the scalar via `numeric_from_int` first (`a + 2` is 5.0, and the
    // scalar comparison converts the 10 the same way).
    let output = clean_output(
        &format!(
            "{FIXED_POINT_STRUCT}extend sys {{\n    \
             run() is also {{\n        \
             var a: fx_t = 3;\n        \
             var d: fx_t = a + 2;\n        \
             out(d);\n        \
             out(a < 10);\n    \
             }};\n}};\n"
        ),
        1,
    );
    assert_eq!(output, "5.0TRUE");
}

#[test]
fn custom_numeric_field_defaults_via_set_initial_value() {
    // T7.1's defaulting rule: a field/variable of a custom numeric
    // type defaults to a fresh value whose `set_initial_value()`
    // defines the state.
    let output = clean_output(
        &format!(
            "{FIXED_POINT_STRUCT}struct holder_s {{ !v: fx_t; }};\n\
             extend sys {{\n    \
             run() is also {{\n        \
             var h: holder_s;\n        \
             out(h.v);\n        \
             out(h.v.as_a(int));\n    \
             }};\n}};\n"
        ),
        1,
    );
    assert_eq!(output, "0.00");
}

#[test]
fn custom_numeric_missing_conversions_report_e0418() {
    // T7.3: a struct without the clause 31 conversion pair reports
    // E0418 where the conversion is needed — a scalar into the type
    // (`numeric_from_int`), a message render (`numeric_to_string`) and
    // an `as_a(int)` (`numeric_to_int`). Each run reports the first
    // missing method the statement sequence reaches.
    let bare = "\
struct bare_s implementing numeric_i of (bare_s, TRUE, 32) {\n\
    !raw: int;\n\
    set_initial_value() is { raw = 0; };\n\
    numeric_assign(other: bare_s) is { raw = other.raw; };\n\
};\n\
numeric_type bare_t: bare_s;\n";
    let from_int = run_source(
        &format!(
            "{bare}extend sys {{\n    \
             run() is also {{ var a: bare_t = 3; }};\n}};\n"
        ),
        1,
    );
    assert_eq!(from_int.diagnostics.len(), 1);
    assert!(
        from_int.diagnostics[0]
            .render()
            .contains("error[E0418]: conversion is not implemented by numeric type 'bare_t' (numeric_from_int missing)"),
        "unexpected diagnostic: {:?}",
        from_int.diagnostics[0].render()
    );
    let to_string = run_source(
        &format!(
            "{bare}extend sys {{\n    \
             run() is also {{ var a: bare_t; out(a); }};\n}};\n"
        ),
        1,
    );
    assert_eq!(to_string.diagnostics.len(), 1);
    assert!(
        to_string.diagnostics[0]
            .render()
            .contains("error[E0418]: output is not implemented by numeric type 'bare_t' (numeric_to_string missing)"),
        "unexpected diagnostic: {:?}",
        to_string.diagnostics[0].render()
    );
    let to_int = run_source(
        &format!(
            "{bare}extend sys {{\n    \
             run() is also {{ var a: bare_t; out(a.as_a(int)); }};\n}};\n"
        ),
        1,
    );
    assert_eq!(to_int.diagnostics.len(), 1);
    assert!(
        to_int.diagnostics[0]
            .render()
            .contains("error[E0418]: conversion to 'int' is not implemented by numeric type 'bare_t' (numeric_to_int missing)"),
        "unexpected diagnostic: {:?}",
        to_int.diagnostics[0].render()
    );
}

#[test]
fn precision_rules_64_bit_unbounded_and_real_contexts() {
    // 5.6.3 (the plan's T7.4 runtime test): rule (c) computes the
    // int(bits:64) comparison unbounded (-1 < 1, TRUE); rule (b)
    // computes the longint/longuint comparison at unsigned 64 bits
    // (-1 casts to MAX_LONGUINT, so `<` is FALSE); rule (d) computes
    // the real + unbounded-int sum in double precision; rule (a)
    // wraps the uint subtraction at 32 unsigned bits. The two `<`
    // results diverging is itself the differential (the plan's C14):
    // the 2019 64-bit rules make `longint -1 < longuint 1` FALSE while
    // the >32-bit rule keeps `int(bits:64) -1 < uint(bits:64) 1` TRUE.
    let output = clean_output(
        "extend sys {\n    \
         run() is also {\n        \
         var x1: int(bits: 64) = -1;  var x2: uint(bits: 64) = 1;\n        \
         out(x1 < x2);\n        \
         var y1: longint = -1;        var y2: longuint = 1;\n        \
         out(y1 < y2);\n        \
         var z1: real = 7.2;          var z2: int(bits: *) = 10;\n        \
         out(z1 + z2);\n        \
         var u: uint = 0;\n        \
         out(u - 1 == MAX_UINT);\n    \
         };\n};\n",
        1,
    );
    assert_eq!(output, "TRUEFALSE17.2TRUE");
}

#[test]
fn precision_rules_wrap_signed_32_bit_results() {
    // 5.6.3's rule (a): the 32-bit signed context truncates the result
    // (an int overflow wraps), while the unbounded path an untyped
    // literal expression runs at keeps the full value.
    let output = clean_output(
        "extend sys {\n    \
         run() is also {\n        \
         var big: int = 2147483647;\n        \
         var wrapped: int = big + 1;\n        \
         out(wrapped == -2147483648);\n        \
         var literal_sum: int = 2147483647 + 1;\n        \
         out(literal_sum);\n    \
         };\n};\n",
        1,
    );
    // `big + 1`: both operands are declared ints — a 32-bit signed
    // context, so the sum wraps to INT_MIN. The literal-only sum is
    // not a variable context (static_type_of does not resolve a
    // literal), so it computes unbounded and prints the full value.
    assert_eq!(output, "TRUE2147483648");
}
