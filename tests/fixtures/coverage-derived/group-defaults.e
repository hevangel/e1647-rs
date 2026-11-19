extend sys {
    event cg;
    !n: bool = FALSE;
    !allow: bool = FALSE;
    cover cg using at_least = 2, weight = 3 is {
        item n: bool using no_collect;
        item flag: bool = FALSE using no_collect;
        cross n, flag;
        transition n using at_least = 1, weight = (1 + 1);
    };
    setup() is also { set_config(cover, mode, normal, at_least, 4); };
    run() is also { emit cg; emit cg; };
};
