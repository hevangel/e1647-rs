extend sys {
    event cg;
    !n: bool = FALSE;
    !allow: bool = FALSE;
    cover cg is {
        item n: bool using no_collect;
        item flag: bool = FALSE using no_collect;
        cross n, flag;
        transition n;
    };
    setup() is also { set_config(cover, mode, normal, at_least, 2); };
    run() is also { emit cg; };
};
