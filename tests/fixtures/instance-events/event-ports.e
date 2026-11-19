unit node_u {
    p: in event_port is instance;
    !hits: uint = 0;
    !hooks: uint = 0;
    worker() @p$ is { hits = hits + 1; };
    on p$ { hooks = hooks + 1; };
};
extend sys {
    a: node_u is instance;
    b: node_u is instance;
    run() is also {
        start a.worker(); start b.worker();
        sync;
        emit a.p$;
        sync;
        check that a.hits == 1;
        check that b.hits == 0;
        emit b.p$;
        sync;
        check that b.hits == 1;
        check that a.hooks == 1;
        check that b.hooks == 1;
        out("event port owners passed");
    };
};
