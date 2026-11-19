struct probe_s {
    event pulse;
    !failures: uint = 0;
    !accepted: bool = TRUE;
    expect not @pulse else { failures = failures + 1; };
    expect eventually true(accepted) else dut_error("late instance not sampled");
};
extend sys {
    run() is also {
        wait [3]*cycle;
        var p: probe_s = new;
        var refs := { p; p; };
        sync;
        emit refs[1].pulse;
        wait [2]*cycle;
        check that p.failures == 1;
        out("dynamic rules passed");
    };
};
