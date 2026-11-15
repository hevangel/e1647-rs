extend sys {
    event ready;
    !calls: uint = 0;
    argument(): uint is { calls = calls + 1; return calls; };
    inner(n: uint) @sys.ready is {
        out("entered ", n, "\n");
        sync;
        out("returned ", n, "\n");
    };
    producer() @sys.any is {
        wait [2]*cycle;
        out("ready\n");
        emit sys.ready;
    };
    run() is also {
        start producer();
        compute inner(argument());
        out("calls ", calls, "\n");
    };
};
