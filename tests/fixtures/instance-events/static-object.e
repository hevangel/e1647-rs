struct node_s {
    event pulse;
    !hits: uint = 0;
    worker() @pulse is { hits = hits + 1; };
};
struct holder_s { static !target: node_s; };
extend sys {
    expect eventually @holder_s::target.pulse else dut_error("missing static object event");
    run() is also {
        start holder_s::target.worker();
        sync;
        emit holder_s::target.pulse;
        wait @holder_s::target.pulse;
        sync;
        check that holder_s::target.hits == 1;
        out("static object event passed");
    };
};
