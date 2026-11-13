extend sys {
    event cg;
    !n: int = 0;
    cover cg is {
        item n: int using ranges = { range([0..1], "", 1); }, illegal = n == 0;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { out("after\n"); };
    check() is also { out("check\n"); };
    finalize() is also { emit cg; out("final\n"); };
};
