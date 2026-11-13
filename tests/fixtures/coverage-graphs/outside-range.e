extend sys {
    event cg;
    !n: bool = FALSE;
    !m: bool = FALSE;
    !allow: bool = TRUE;
    !value: int = 0;
    cover cg is {
        item v: int = value using no_collect, ranges = { range([0..1], "", 1); };
        item m: bool using no_collect;
        cross v, m using name = "p", no_collect;
        transition p;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; value = 2; emit cg; value = 1; emit cg; value = 0; emit cg; };
};
