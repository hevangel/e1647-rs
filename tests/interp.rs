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
