extend sys {
    event cg;
    !n: bool = FALSE;
    cover cg is {
        item n: bool using no_collect;
        item flag: bool = FALSE using no_collect;
        cross n, flag;
    };
    cover cg using at_least = 2, weight = 4 is also {
        transition n using at_least = 1;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; emit cg; };
};
