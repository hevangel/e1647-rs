extend sys {
    finish(): uint is { sync; out("stopping\n"); stop_run(); return 99; };
    run() is also {
        var value: uint = finish();
        out("unreachable\n");
    };
};
