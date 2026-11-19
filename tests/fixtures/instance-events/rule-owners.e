unit node_u {
    event ready;
    !failed: uint = 0;
    !id: uint = 0;
    !level: uint = 0;
    expect eventually @ready else { failed = failed + 1; };
    expect eventually true(level == id) else dut_error("wrong expression owner");
};
struct never_created_s { expect eventually @missing else dut_error("uninstantiated rule"); };
extend sys {
    a: node_u is instance;
    b: node_u is instance;
    run() is also {
        a.id = 1; b.id = 2;
        a.level = 1; b.level = 2;
        emit a.ready;
        wait [2]*cycle;
    };
    finalize() is also {
        check that a.failed == 0;
        check that b.failed == 1;
        out("rule owners passed");
    };
};
