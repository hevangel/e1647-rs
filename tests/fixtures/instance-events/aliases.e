unit channel_u {
    event pulse;
    !hits: uint = 0;
    entry() @me.pulse is { hits = hits + 1; };
};
extend sys {
    a: channel_u is instance;
    b: channel_u is instance;
    !calls: uint = 0;
    !indexes: uint = 0;
    receiver(): channel_u is { calls = calls + 1; return a; };
    index(): uint is { indexes = indexes + 1; return 0; };
    producer() @sys.any is { wait [2]*cycle; emit b.pulse; };
    run() is also {
        start a.entry(); start b.entry();
        sync;
        emit receiver().pulse;
        sync;
        check that a.hits == 1;
        check that b.hits == 0;
        check that calls == 1;
        var alias := b;
        start producer();
        wait @alias.pulse;
        sync;
        check that b.hits == 1;
        emit { a; b; }[index()].pulse;
        check that indexes == 1;
        out("event aliases passed");
    };
};
