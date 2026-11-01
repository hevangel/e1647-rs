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
    // 13.1.1: `start` enqueues the TCM with an implicit sync cycle and
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

#[test]
fn temporal_wait_in_a_tcm_observes_emissions_made_while_suspended() {
    // The marquee clause 13.1.1 behavior: the waiter suspends at
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
    // 13.1.2: `sync;` suspends for one occurrence of the default
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
    // Native string matching (clause 4.11.1) is case insensitive and
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
    // `/.../` selects AWK-style regular expressions (clause 4.11.2) and
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
    let result = run(
        &elaborate(
            &parse(
                "struct s {\n    v: int;\n};\n\
             extend sys {\n    run() is also {\n        var p: s = new;\n    };\n};\n",
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
    // wait started in the same cycle succeeds immediately (12.3.1's
    // sub-path over the recorded state).
    let output = clean_output(
        "extend sys {\n    ev: event;\n    run() is also {\n        emit ev;\n        \
         wait @ev;\n        out(\"WAIT DONE\");\n    };\n};\n",
        1,
    );
    assert!(output.contains("WAIT DONE"), "output: {output}");
}

#[test]
fn temporal_wait_sees_events_recorded_by_on_blocks() {
    // The `on` block's numeric wait advanced the cycle and the emission
    // landed in the new cycle's state; the temporal wait `until @ev`
    // (13.1.3's readability-only `until`) succeeds against the log.
    let output = clean_output(
        "extend sys {\n    ev: event;\n    on reset_ev {\n        wait [1]*cycle;\n        \
         emit ev;\n    };\n    reset_ev: event;\n    run() is also {\n        emit reset_ev;\n        \
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
    // rule: its lhs never occurs, so nothing is obligated (12.2.13).
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
    // `wait {@a; cycle}` (12.2.8): a occurs in the start cycle; the
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
    // 14.1/14.2: the group samples when its event fires; each item
    // value lands in the bucket whose range contains it, and values
    // outside every bucket are ungradeable.
    let result = run_source(
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
    // 14.2: integers default to 16 buckets (0 through 15), booleans to
    // their two values; out-of-range values do not affect the grade.
    let result = run_source(
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
    // 14.5: `is also` extensions add items to the base group; an
    // `empty` placeholder receives them too.
    let result = run_source(
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
    // recorded but ungradeable (14.2: goals of 0).
    let result = run_source(
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
