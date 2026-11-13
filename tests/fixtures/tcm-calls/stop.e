extend sys {
    inner() is { sync; out("stop\n"); stop_run(); out("unreachable\n"); };
    outer() is { compute inner(); out("unreachable\n"); };
    run() is also { outer(); out("unreachable\n"); };
};
