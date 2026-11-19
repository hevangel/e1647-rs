unit node_u {
    event sample_e;
    !level: uint = 0;
    !done: uint = 0;
    expect eventually (rise(level) @sample_e) else dut_error("missing own rise");
    worker() @sys.any is { wait rise(level) @me.sample_e; done = done + 1; };
    fire() is { emit sample_e; };
};
extend sys {
    a: node_u is instance;
    b: node_u is instance;
    run() is also {
        start a.worker(); start b.worker();
        sync;
        a.fire(); b.fire();
        sync;
        a.level = 1; a.fire();
        sync;
        check that a.done == 1;
        check that b.done == 0;
        b.level = 2; b.fire();
        sync;
        check that b.done == 1;
        out("edge owners passed");
    };
};
