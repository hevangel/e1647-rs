extend sys {
    event cg;
    !n: bool = FALSE;
    !allow: bool = FALSE;
    cover cg is {
        item n: bool using per_instance;
        item flag: bool = FALSE using weight = 9;
        cross n, flag using weight = 3;
        transition n using when = FALSE, weight = 1;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; };
};
