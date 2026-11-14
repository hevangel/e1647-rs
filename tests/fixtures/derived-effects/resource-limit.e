extend sys {
    event cg;
    !n: int = 0;
    !m: int = 0;
    cover cg is {
        item n: int using no_collect, ranges = { range([0..100000]); };
        item m: int using no_collect, ranges = { range([0..100000]); };
        cross n, m using ignore = n % 2 == 0 or n % 2 == 1;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; };
};
