struct matcher_s {
    min(v: int): int is { return v + 1; };
    max(v: int): int is { return v + 2; };
    min_index(v: int): int is { return v + 3; };
    max_index(v: int): int is { return v + 4; };
};
extend sys {
    !calls: uint = 0;
    m: matcher_s;
    receiver(): matcher_s is { calls += 1; return m; };
    run() is also {
        out(receiver().min(4), " ", calls, "\n");
        out(receiver().max(4), " ", calls, "\n");
        out(receiver().min_index(4), " ", calls, "\n");
        out(receiver().max_index(4), " ", calls, "\n");
    };
};
