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
fn temporal_wait_sees_events_recorded_by_a_tcm() {
    // A started TCM's numeric wait advanced the cycle and the emission
    // landed in the new cycle's state; the temporal wait `until @ev`
    // (13.1.3's readability-only `until`) succeeds against the log.
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

#[test]
fn select_weights_bias_the_sampler_to_min_and_max() {
    // 10.2.12.2: an arm is chosen with probability weight/(sum of all
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
    // 6.6 + 10.2.12.2 together: a when-qualified field (`LO pkt`)
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
    // 6.6 chained: a chained-qualified field (`LO HI pkt`) carries the
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
    // 10.2.7's connected field sets, v1: `gen p` inside a when-view
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
    // 14.4: the transition derives previous-value/value samples from
    // the base item's consecutive samples; gradeable over the base's
    // bucket pairs.
    let result = run_source(
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
    // n=3 base samples → n-1=2 transition samples (14.4).
    assert_eq!(transition.samples, 2);
    // low→high and high→low observed of the 4 ordered bucket pairs.
    assert_eq!(transition.combos.len(), 2);
    assert_eq!(transition.total_combos, Some(4));
    assert_eq!(transition.grade(), Some(0.5));
    assert_eq!(group.grade(), Some((1.0 + 0.5) / 2.0));
}

#[test]
fn cross_items_collect_combinations_of_their_sources() {
    // 14.3: the cross shows every combination of the crossed items'
    // values; gradeable over the cross product of the bucket counts.
    let result = run_source(
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
    let result = run_source(
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
    // 5.8.4: the set of all legal values of a numeric or enumeral
    // type — scalars from their width and signedness, enumerals as
    // their indices.
    let output = clean_output(
        "type addr_t : int (bits : 8);\n\
         type color_t : [WHITE, BLACK, RED];\n\
         extend sys {\n    run() is also {\n            print set_of_values(addr_t);\n            \
         print set_of_values(color_t);\n            print set_of_values(bit);\n        };\n};\n",
        1,
    );
    assert!(output.contains("[-128..127]"), "output: {output}");
    assert!(output.contains("[0..2]"), "output: {output}");
    assert!(output.contains("[0..1]"), "output: {output}");
}

#[test]
fn set_pseudo_methods_query_and_combine() {
    // 27.4: size/uint_size/min/uint_min/max/uint_max on sets, and the
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
    // 27.4.1-27.4.3: the algebra results are canonical (5.1.10:
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
    // 5.8.1: as_a to an enumeral type maps the integer value to the
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
    // 11.3 + 9.4.1: `on p$` runs its block when the event port's event
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
    // 14.2.1: each bucket of a per_instance item is its own graded
    // instance (`item==bucket`); unsampled instances are holes; the
    // group grade is the weighted grades of the instances plus the
    // per_type bucket.
    let result = run_source(
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
    // 14.2.1: "any instance that is not sampled is marked as a hole" —
    // a hole grades 0 and pulls the group grade down.
    let result = run_source(
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
    let result = run_source(
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
    let result = run_source(
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
    // 14.1's NOTE: "Unless coverage mode is turned on first (see 28.9),
    // no coverage results are collected, even if cover groups and cover
    // items are defined." With the mode off (28.9's set_config() is not
    // wired, so the configuration is the builder API) nothing is built.
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
    // The default configuration keeps collection on (the documented v1
    // deviation: the standard's mode starts off until set_config turns
    // it on, and the routine is not wired yet).
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
    // 14.2: "If ranges is not specified, the default number of buckets
    // is 16 [set by the max_int_buckets coverage configuration option
    // (see 28.9)]."
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
    // 14.1.1: "if one or both of no_collect and instance_no_collect
    // options of the group are evaluated to TRUE, the group shall not
    // be presented under that instance." The constant TRUE spelling is
    // not presented at all — not sampled, not graded, not reported.
    let result = run_source(
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
    // 14.1.1's OR relation: instance_no_collect = TRUE alone (no
    // no_collect on the group) suppresses presentation the same way.
    let result = run_source(
        "extend sys {\n    event cov_e;\n    cur: uint;\n    cover cov_e is {\n        \
         instance_no_collect = TRUE;\n        item cur: uint;\n    };\n    run() is also {\n        \
         cur = 0; emit cov_e;\n    };\n};\n",
        1,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.coverage[0].groups.is_empty());
}

#[test]
fn no_collect_item_option_suppresses_the_item() {
    // 14.2.3: "if one or both of no_collect and instance_no_collect
    // options of the item are evaluated to TRUE, the item shall not be
    // presented under that instance" — the item drops from the group;
    // its siblings still collect.
    let result = run_source(
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
    // 14.3/14.4: a cross or transition "creates a new item with a name
    // specified using a name option" instead of the default
    // cross_a__b / transition__base names.
    let result = run_source(
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
    // 14.8.2/14.8.3: weights and the at_least minimum "can be specified
    // in the coverage group or item definitions" — the group option
    // applies to every item that does not set its own; the item-level
    // spelling wins where present.
    let result = run_source(
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
    // 20.2.2: the body executes at least once, then repeatedly until
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
    // 21.2's `#else`: the alternative group runs when the condition
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
