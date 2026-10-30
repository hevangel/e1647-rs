// Mixed e + SystemVerilog testcase, signal-map style: a unit declares
// `simple_port` members bound with `hdl_path()` keeps, and the e code
// accesses them with the `port$` syntax (the SN-CPU-Tutorial pattern).
// The DUT (examples/mixed/counter.sv) is simulated by xezim through the
// VPI bridge.

unit smp_u {
    clk_p: inout simple_port of bit is instance;
    keep clk_p.hdl_path() == "counter.clk";
    rst_p: inout simple_port of bit is instance;
    keep rst_p.hdl_path() == "counter.rst";
    count_p: inout simple_port of uint(bits:8) is instance;
    keep count_p.hdl_path() == "counter.count";
};

extend sys {
    smp: smp_u is instance;
    run() is also {
        hdl_load("examples/mixed/counter.sv", "counter");

        // Reset the counter asynchronously.
        smp.clk_p$ = 0;
        smp.rst_p$ = 1;
        hdl_run(1);
        smp.rst_p$ = 0;

        var expected: uint = 1;
        var cycles: uint = 5;
        var i: uint = 0;
        while i < cycles {
            // Rising edge: the counter increments.
            smp.clk_p$ = 1;
            hdl_run(1);
            var seen: uint = smp.count_p$;
            if seen != expected {
                out("MISMATCH at cycle ", i, ": saw ", seen,
                    " expected ", expected, "\n");
            } else {
                out("cycle ", i, ": count=", seen, " OK\n");
            };
            expected = expected + 1;
            i = i + 1;
            // Falling edge back at zero before the next cycle.
            smp.clk_p$ = 0;
            hdl_run(1);
        };

        hdl_finish();
        out("port testcase complete\n");
    };
};
