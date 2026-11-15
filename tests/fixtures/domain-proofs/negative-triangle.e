extend sys {
    event cg;
    !n: int = 0;
    !m: int = 0;
    cover cg is {
        item n: int using no_collect, ranges = { range([-1000000000..-1]); };
        item m: int using no_collect, ranges = { range([-1000000000..-1]); };
        cross n, m using ignore = n <= m;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { n = -2; m = -3; emit cg; };
};
