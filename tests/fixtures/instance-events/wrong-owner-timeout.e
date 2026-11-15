unit channel_u {
    event pulse;
    worker() @sys.any is {
        wait @pulse;
    };
};
extend sys {
    a: channel_u is instance;
    b: channel_u is instance;
    driver() @sys.any is { wait [2]*cycle; emit b.pulse; };
    run() is also { start a.worker(); start driver(); };
};
