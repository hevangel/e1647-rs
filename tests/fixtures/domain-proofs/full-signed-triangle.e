extend sys {
    event cg;
    !n: int = 0;
    !m: int = 0;
    cover cg is {
        item n: int using no_collect, ranges = { range([(-170141183460469231731687303715884105727 - 1)..170141183460469231731687303715884105727]); };
        item m: int using no_collect, ranges = { range([(-170141183460469231731687303715884105727 - 1)..170141183460469231731687303715884105727]); };
        cross n, m using ignore = n <= m;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; n = 1; emit cg; };
};
