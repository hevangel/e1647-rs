extend sys {
    event cg;
    !n: int = 0;
    !m: int = 0;
    cover cg is {
        item n: int using no_collect, ranges = { range([0..1000000000]); };
        item m: int using no_collect, ranges = { range([0..1000000000]); };
        cross n, m using ignore = n <= m;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; n = 1; emit cg; m = 1; emit cg; n = 2; emit cg; };
};
