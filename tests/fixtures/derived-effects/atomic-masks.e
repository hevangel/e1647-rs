extend sys {
    event cg;
    !n: int = 0;
    !m: int = 0;
    cover cg is {
        item n: int using no_collect, ranges = { range([0..100000]); };
        item m: int using no_collect, ranges = { range([0..100000]); };
        cross n, m using name = "first", ignore = TRUE;
        cross n, m using name = "second", ignore = n == n;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; };
};
