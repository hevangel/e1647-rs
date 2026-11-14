extend sys {
    event cg;
    !n: int = 0;
    !m: int = 0;
    cover cg is {
        item n: int using no_collect, ranges = { range([0..1000000000]); };
        item m: int using no_collect, ranges = { range([0..1000000000]); };
        cross n, m using ignore = n in [m..m+3];
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { m = 1; emit cg; };
};
