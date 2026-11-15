extend sys {
    event ready;
    !calls: uint = 0;
    !value: uint = 0;
    prefix(): uint is { calls = calls + 1; return calls; };
    pending(): uint is {
        wait @ready;
        return 7;
    };
    producer() @sys.any is { wait [2]*cycle; emit ready; };
    run() is also {
        start producer();
        value = prefix() + pending();
        out("result ", value, " calls ", calls, "\n");
    };
};
