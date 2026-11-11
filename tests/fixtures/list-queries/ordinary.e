struct matcher_s {
    has(v: int): int is { return v + 1; };
    count(v: int): int is { return v + 2; };
    all(v: int): int is { return v + 3; };
    all_indices(v: int): int is { return v + 4; };
};
extend sys {
    !calls: uint = 0;
    m: matcher_s;
    receiver(): matcher_s is { calls += 1; return m; };
    run() is also {
        out(receiver().has(4), " ", calls, "\n");
        out(receiver().count(4), " ", calls, "\n");
        out(receiver().all(4), " ", calls, "\n");
        out(receiver().all_indices(4), " ", calls, "\n");
    };
};
