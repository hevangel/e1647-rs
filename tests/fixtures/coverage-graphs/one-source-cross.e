extend sys {
    event cg;
    !n: bool = FALSE;
    !m: bool = FALSE;
    !allow: bool = TRUE;
    cover cg is {
        item n: bool using no_collect;
        item m: bool using no_collect;
        cross n;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also {  };
};
