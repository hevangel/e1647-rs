extend sys {
    event cg;
    !n: bool = FALSE;
    !allow: bool = FALSE;
    cover cg is {
        item n: bool using no_collect;
        item flag: bool = FALSE using no_collect;
        cross n, flag using when = allow;
        transition n using when = allow;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { var allow: bool = TRUE; emit cg; emit cg; };
};
