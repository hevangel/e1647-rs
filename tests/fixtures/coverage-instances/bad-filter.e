extend sys {
    event cg;
    cover cg using when = 7 is { item n: bool = TRUE; };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; };
};
