extend sys {
    event cg;
    !n: bool = FALSE;
    !m: bool = FALSE;
    !allow: bool = TRUE;
    cover cg is {
        item n: bool using no_collect;
        item m: bool using no_collect;
        transition n using name = "t", no_collect;
        cross t, m;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; n = TRUE; emit cg; n = FALSE; m = TRUE; emit cg; n = TRUE; emit cg; };
};
