extend sys {
    event cg;
    !n: int = 0;
    !m: int = 0;
    next_value(): int is { n += 1; return n; };
    cover cg is {
        item value: int = next_value() using no_collect, ranges = { range([0..3], "", 1); };
        transition value using ignore = value == prev_value;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; emit cg; out(n, "\n"); };
};
