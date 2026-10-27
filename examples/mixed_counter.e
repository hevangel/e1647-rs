// Mixed e + SystemVerilog testcase: the DUT (examples/mixed/counter.sv)
// is simulated by xezim; the e code drives its inputs and checks its
// outputs through the VPI bridge (src/hdl.rs, compose `mixed` service).
//
// Signal access uses the asic-world quoted-path style
// (`'counter.clk' = 1`), which the interpreter maps onto the simulator
// bridge; `hdl_load`/`hdl_finish` manage the session.

extend sys {
    run() is also {
        hdl_load("examples/mixed/counter.sv", "counter");

        // Reset the counter asynchronously.
        'counter.clk' = 0;
        'counter.rst' = 1;
        hdl_run(1);
        'counter.rst' = 0;

        // After reset, the first rising edge increments the counter from
        // 0 to 1 — the reads below happen after each incrementing edge.
        var expected: uint = 1;
        var cycles: uint = 5;
        var i: uint = 0;
        while i < cycles {
            // Rising edge: the counter increments.
            'counter.clk' = 1;
            hdl_run(1);
            var seen: uint = 'counter.count';
            if seen != expected {
                out("MISMATCH at cycle ", i, ": saw ", seen,
                    " expected ", expected, "\n");
            } else {
                out("cycle ", i, ": count=", seen, " OK\n");
            };
            expected = expected + 1;
            i = i + 1;
            // Falling edge back at zero before the next cycle.
            'counter.clk' = 0;
            hdl_run(1);
        };

        hdl_finish();
        out("mixed testcase complete\n");
    };
};
