extend sys {
    event cg;
    !n: int = 0;
    !m: int = 0;
    cover cg is {
        item n: int using no_collect, ranges = { range([0..2]); };
        transition n using ignore = n == prev_n, at_least = 2;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; emit cg; n = 1; emit cg; emit cg; n = 2; emit cg; };
};
