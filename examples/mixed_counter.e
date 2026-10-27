// Mixed e + SystemVerilog testcase: the DUT (examples/mixed/counter.sv)
// is simulated by xezim; the e code drives its inputs and checks its
// outputs through the VPI bridge (src/hdl.rs, compose `mixed` service).
//
// The e engine stays the master of verification: it applies reset,
// clocks the design, reads `count` after every posedge and compares it
// with the expected sequence — the same structure a Specman testcase
// would have with an SV DUT.

extend sys {
    run() is also {
        hdl_load("examples/mixed/counter.sv", "counter");

        // Reset the counter asynchronously.
        hdl_set("counter.clk", 0);
        hdl_set("counter.rst", 1);
        hdl_run(1);
        hdl_set("counter.rst", 0);

        // After reset, the first rising edge increments the counter from
        // 0 to 1 — the reads below happen after each incrementing edge.
        var expected: uint = 1;
        var cycles: uint = 5;
        var i: uint = 0;
        while i < cycles {
            // Rising edge: the counter increments.
            hdl_set("counter.clk", 1);
            hdl_run(1);
            var seen: uint = hdl_get("counter.count");
            if seen != expected {
                out("MISMATCH at cycle ", i, ": saw ", seen,
                    " expected ", expected, "\n");
            } else {
                out("cycle ", i, ": count=", seen, " OK\n");
            };
            expected = expected + 1;
            i = i + 1;
            // Falling edge back at zero before the next cycle.
            hdl_set("counter.clk", 0);
            hdl_run(1);
        };

        hdl_finish();
        out("mixed testcase complete\n");
    };
};
