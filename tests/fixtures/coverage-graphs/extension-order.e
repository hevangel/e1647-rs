extend sys {
    event cg;
    !n: bool = FALSE;
    !m: bool = FALSE;
    cover cg is also { transition p; };
    cover cg is {
        item n: bool using no_collect;
        item m: bool using no_collect;
        cross n, m using name = "p", no_collect;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; n = TRUE; emit cg; };
};
