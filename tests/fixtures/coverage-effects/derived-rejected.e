extend sys {
    event cg;
    !n: int = 0;
    cover cg is {
        item n: int using ranges = { range([0..3], "", 1); }, ignore = n in [0, 2], per_instance;
        item flag: bool = FALSE;
        cross n, flag using ignore = TRUE;
        transition n;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { for i from 0 to 3 { n = i; emit cg; }; };
};

