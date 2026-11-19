extend sys {
    event ready;
    !calls: uint = 0;
    argument(): uint is { calls = calls + 1; return calls; };
    pending(n: uint): uint @sys.ready is { sync; return n + 7; };
    producer() @sys.any is { wait [2]*cycle; emit ready; };
    run() is also {
        start producer();
        var answer: uint = pending(argument());
        out(answer, " calls ", calls, "\n");
    };
};
