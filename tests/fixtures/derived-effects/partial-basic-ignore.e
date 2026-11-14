extend sys {
    event cg;
    !n: int = 0;
    !m: int = 0;
    cover cg is {
        item n: int using no_collect, ranges = { range([0..2]); }, ignore = n == 1;
        item m: int using no_collect, ranges = { range([0..2]); };
        cross n, m using ignore = n != 1;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; n = 1; emit cg; m = 1; emit cg; n = 0; emit cg; };
};
