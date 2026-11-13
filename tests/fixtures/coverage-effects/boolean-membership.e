extend sys {
    event cg;
    !n: bool = FALSE;
    cover cg is { item n: bool using ignore = n in [FALSE]; };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; n = TRUE; emit cg; };
};

