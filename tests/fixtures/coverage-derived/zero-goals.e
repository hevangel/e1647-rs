extend sys {
    event cg;
    !n: bool = FALSE;
    !allow: bool = FALSE;
    cover cg is {
        item n: bool using no_collect;
        item flag: bool = FALSE using no_collect;
        cross n, flag using at_least = 0;
        transition n using at_least = 0;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; emit cg; };
};
