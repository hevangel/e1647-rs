extend sys {
    q: event;
    !level: uint = 0;
    worker(tag: uint) @sys.any is {
        wait rise(level) @q;
        out(tag, " ", level, "\n");
    };
    driver() @sys.any is {
        emit q;
        sync;
        sync;
        level = 2;
        emit q;
    };
    run() is also {
        start worker(1);
        start worker(2);
        start driver();
        wait [6]*cycle;
        out("complete\n");
    };
};
