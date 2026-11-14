extend sys {
    event cg;
    !n: bool = FALSE;
    !allow: bool = FALSE;
    cover cg is {
        item n: bool using no_collect;
        item flag: bool = FALSE using no_collect;
        cross n, flag using when = allow, at_least = 2;
        transition n using when = allow, at_least = 2;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; n = TRUE; emit cg; allow = TRUE; n = FALSE; emit cg;
        n = TRUE; emit cg; allow = FALSE; n = FALSE; emit cg; allow = TRUE; n = TRUE; emit cg; };
};
