extend sys {
    event cg;
    !n: bool = FALSE;
    !m: bool = FALSE;
    !allow: bool = TRUE;
    !payload: real = 1.25;
    cover cg is {
        item word: real = payload using no_collect;
        item m: bool using no_collect;
        cross word, m using name = "p", no_collect;
        transition p;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; payload = 2.5; emit cg; payload = 1.25; emit cg; };
};
