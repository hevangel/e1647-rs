extend sys {
    event cg;
    !n: int = 0;
    !m: int = 0;
    cover cg is {
        item n: int using no_collect, ranges = { range([0..2]); };
        transition n using ignore = prev_n == 999;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { var prev_n: int = 999; emit cg; n = 1; emit cg; n = 2; emit cg; };
};
