extend sys {
    !phase: uint = 0;
    driver() @sys.any is {
        phase = 1;
        sync;
        phase = 2;
        sync;
        phase = 3;
    };
    run() is also {
        start driver();
        wait {true(phase == 1); true(phase == 2)};
        out("wait complete\n");
    };
};
