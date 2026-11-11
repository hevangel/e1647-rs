extend sys {
    !phase: uint = 0;
    worker(expected: uint) @sys.any is {
        wait {true(phase == expected); true(phase == expected + 1)};
        out(expected, "\n");
    };
    driver() @sys.any is {
        phase = 1;
        sync;
        phase = 2;
        sync;
        phase = 3;
    };
    run() is also {
        start worker(1);
        start worker(2);
        start driver();
        wait [6]*cycle;
        out("complete\n");
    };
};
