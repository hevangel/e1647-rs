extend sys {
    event cg;
    !n: bool = FALSE;
    cover cg is { item n: bool using per_instance, at_least = 0; };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; };
};
