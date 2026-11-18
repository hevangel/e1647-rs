unit channel_u {
    event sample_e;
    !accepted: bool = FALSE;
    !finished: bool = FALSE;
    cover sample_e using per_unit_instance is { item accepted: bool; };
    record(incoming: bool) is {
        sync;
        accepted = evaluate { value = incoming; };
        emit sample_e;
    };
    worker(incoming: bool) @sys.any is {
        record(incoming);
        record(not incoming);
        finished = TRUE;
    };
};
extend sys {
    a: channel_u is instance;
    b: channel_u is instance;
    setup() is also { set_config(cover, mode, normal); };
    run() is also {
        start a.worker(evaluate { value = TRUE; });
        start b.worker(evaluate { value = FALSE; });
        wait [8]*cycle;
        check that a.finished;
        check that b.finished;
        out("coverage owner contexts passed");
    };
};
