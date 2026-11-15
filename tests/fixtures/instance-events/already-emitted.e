unit channel_u {
    event pulse;
    !hits: uint = 0;
    entry() @pulse is { hits = hits + 1; };
};
extend sys {
    a: channel_u is instance;
    b: channel_u is instance;
    run() is also {
        emit a.pulse;
        start a.entry(); start b.entry();
        sync;
        check that a.hits == 1;
        check that b.hits == 0;
        emit b.pulse;
        sync;
        check that b.hits == 1;
        out("entry owners passed");
    };
};
