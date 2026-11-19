extend sys {
    event cg;
    cover cg is {
        item word: string = "a" using no_collect;
        item n: bool = FALSE using no_collect, ignore = TRUE;
        cross word, n using name = "p", no_collect;
        transition p;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; };
};
