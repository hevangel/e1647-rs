extend sys {
    event cg;
    !n: int = 0;
    !m: int = 0;
    cover cg is {
        item n: int using no_collect, ranges = { range([0..1], "", 1); };
        item m: int using no_collect, ranges = { range([0..1], "", 1); };
        transition n using ignore = n in [prev_n];
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; emit cg; n = 1; emit cg; };
};
