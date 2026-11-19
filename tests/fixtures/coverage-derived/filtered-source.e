extend sys {
    event cg;
    !n: bool = FALSE;
    !allow: bool = FALSE;
    cover cg is {
        item n: bool using no_collect;
        item flag: bool = FALSE using no_collect, when = allow;
        cross n, flag using when = TRUE;
        transition n using when = TRUE;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { allow = TRUE; emit cg; allow = FALSE; n = TRUE; emit cg; };
};
