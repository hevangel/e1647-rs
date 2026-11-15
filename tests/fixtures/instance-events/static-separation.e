struct bus_s {
    static event pulse;
    static !hits: uint = 0;
    static on pulse { hits = hits + 1; };
};
unit node_u {
    event pulse;
    !hits: uint = 0;
    worker() @pulse is { hits = hits + 1; };
};
extend sys {
    a: node_u is instance;
    !hits: uint = 0;
    expect eventually @bus_s::pulse else dut_error("missing static event");
    worker() @bus_s::pulse is { hits = hits + 1; };
    run() is also {
        start a.worker(); start worker();
        sync;
        emit a.pulse;
        sync;
        check that a.hits == 1;
        check that hits == 0;
        emit (bus_s::pulse);
        wait @bus_s::pulse;
        sync;
        check that hits == 1;
        check that bus_s::hits == 1;
        out("static identities passed");
    };
};
