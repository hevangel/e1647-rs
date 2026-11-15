extend sys {
    event cg;
    !n: bool = FALSE;
    !m: bool = FALSE;
    !allow: bool = TRUE;
    cover cg is {
        item n: bool using no_collect;
        item m: bool using no_collect;
        transition n using name = "p";
        cross n, m using name = "p";
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also {  };
};
