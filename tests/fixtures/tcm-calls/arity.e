extend sys {
    event never;
    inner() @sys.never is { out("unreachable\n"); };
    run() is also { compute inner(1); };
};
