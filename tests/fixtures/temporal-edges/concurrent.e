extend sys {
    !level: uint = 0;
    worker(offset: uint) @sys.any is {
        wait change(level + offset);
        out(offset, " ", level, "\n");
    };
    driver() @sys.any is {
        sync;
        sync;
        level = 1;
    };
    run() is also {
        start worker(0);
        start worker(10);
        start driver();
        wait [6]*cycle;
        out("complete\n");
    };
};
