extend sys {
    pending(): uint is { sync; return 1 / 0; };
    run() is also {
        var answer: uint = pending();
        out("unreachable\n");
    };
};
