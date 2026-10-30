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
fn start_runs_tcm_inline_and_waits_advance() {
    let output = clean_output(
        "unit counter_u {\n    ticks: uint;\n    tick() @sys.any is {\n        var i: uint = 0;\n        while i < 3 {\n            i = i + 1;\n            ticks = ticks + 1;\n            wait cycle;\n        };\n    };\n};\nextend sys {\n    c: counter_u is instance;\n    run() is also {\n        start c.tick();\n        print c.ticks;\n        emit sys.done;\n    };\n};\n",
        1,
    );
    // The inline-started TCM mutated the instance across its waits.
    assert!(output.contains("3\n"), "output: {output}");
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
