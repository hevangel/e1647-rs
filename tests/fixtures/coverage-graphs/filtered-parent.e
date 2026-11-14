extend sys {
    event cg;
    !n: bool = FALSE;
    !m: bool = FALSE;
    !allow: bool = TRUE;
    cover cg is {
        item n: bool using no_collect;
        item m: bool using no_collect;
        cross n, m using name = "p", no_collect, when = allow;
        transition p;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; allow = FALSE; n = TRUE; emit cg; allow = TRUE; n = FALSE; m = TRUE; emit cg; n = TRUE; emit cg; };
};
