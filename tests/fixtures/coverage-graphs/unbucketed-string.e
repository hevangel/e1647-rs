extend sys {
    event cg;
    !n: bool = FALSE;
    !m: bool = FALSE;
    !allow: bool = TRUE;
    !payload: string = "left";
    cover cg is {
        item word: string = payload using no_collect;
        item m: bool using no_collect;
        cross word, m using name = "p", no_collect;
        transition p;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; payload = "right"; emit cg; payload = "left"; emit cg; };
};
