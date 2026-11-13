extend sys {
    !gate: semaphore;
    inner(n: uint) is {
        var saved: uint = n;
        gate.down();
        sync;
        out(saved, "\n");
    };
    outer(n: uint) @sys.any is {
        var saved: uint = n + 10;
        inner(n);
        out(saved, "\n");
    };
    producer() @sys.any is {
        wait [2]*cycle;
        gate.up();
        gate.up();
    };
    run() is also {
        gate = new semaphore;
        start outer(1);
        start outer(2);
        start producer();
        wait [10]*cycle;
        out("done\n");
    };
};
