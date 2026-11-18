struct helper_s {
    gate: semaphore;
    take(n: uint) is {
        var saved: uint = n + 10;
        out("enter ", n, "\n");
        gate.down();
        out("leave ", saved, "\n");
    };
};
extend sys {
    !h: helper_s;
    !calls: uint = 0;
    arg(): uint is { calls = calls + 1; return calls; };
    outer(n: uint) is {
        var saved: uint = n + 100;
        compute h.take(n);
        sync;
        out("outer ", saved, "\n");
    };
    producer() @sys.any is {
        wait [2]*cycle;
        out("release\n");
        h.gate.up();
    };
    run() is also {
        h = new helper_s;
        h.gate = new semaphore;
        start producer();
        outer(arg());
        out("done ", calls, "\n");
    };
};
