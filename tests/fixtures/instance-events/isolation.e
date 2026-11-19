unit channel_u {
    event pulse;
    !hits: uint = 0;
    entry() @pulse is { hits = hits + 1; };
    waiter() @sys.any is { wait @self.pulse; hits = hits + 1; };
    fire() is { emit pulse; };
};
extend sys {
    a: channel_u is instance;
    b: channel_u is instance;
    run() is also {
        start a.entry(); start b.entry();
        start a.waiter(); start b.waiter();
        wait [2]*cycle;
        a.fire();
        sync;
        check that a.hits == 2;
        check that b.hits == 0;
        var alias := b;
        emit alias.pulse;
        sync;
        check that b.hits == 2;
        out("instance isolation passed");
    };
};
