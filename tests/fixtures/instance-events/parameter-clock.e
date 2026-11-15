unit channel_u { event pulse; };
extend sys {
    a: channel_u is instance;
    b: channel_u is instance;
    !hits: uint = 0;
    worker(p: channel_u) @p.pulse is { hits = hits + 1; };
    producer() @sys.any is { wait [2]*cycle; emit a.pulse; };
    run() is also {
        start worker(a); start worker(b);
        sync;
        emit a.pulse;
        sync;
        check that hits == 1;
        emit b.pulse;
        sync;
        check that hits == 2;
        start producer();
        compute worker(a);
        check that hits == 3;
        wait @sys.any;
        out("entry parameter clocks passed");
    };
};
