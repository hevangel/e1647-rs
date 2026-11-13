extend sys {
    event cg;
    !n: bool = FALSE;
    !allow: bool = FALSE;
    cover cg is {
        item n: bool using no_collect;
        item flag: bool = FALSE using no_collect;
        cross n, flag using weight = 0.5;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also {  };
};
