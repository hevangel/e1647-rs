extend sys {
    !calls: uint = 0;
    later(n: uint): uint is { calls = calls + 1; sync; return n; };
    sink(n: uint) @sys.any is { sync; out("sink ", n, "\n"); };
    run() is also {
        compute sink(later(1));
        start sink(later(2));
        wait [later(2)]*cycle;
        out("calls ", calls, "\n");
    };
};
