extend sys {
    event cg;
    !n: int = 0;
    !m: int = 0;
    cover cg is {
        item n: int using no_collect, ranges = { range([0..1], "", 1); };
        item m: int using no_collect, ranges = { range([0..1], "", 1); };
        cross n, m using illegal = n == m;
    };
    setup() is also { set_config(cover, mode, normal, check_illegal_immediately, TRUE); };
    run() is also { emit cg; n = 1; emit cg; m = 1; emit cg; n = 0; emit cg; out("done\n"); };
    check() is also { out("check\n"); };
    finalize() is also { out("end\n"); };
};
