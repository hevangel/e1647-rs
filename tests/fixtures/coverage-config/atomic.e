extend sys {
    event cg;
    !n: uint = 0;
    cover cg is { item n: uint; };
    setup() is also { set_config(cover, mode, normal, unknown_option, 3); };
    run() is also { emit cg; out("continued\n"); };
};
