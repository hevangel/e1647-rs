extend sys {
    event cg;
    !n: bool = FALSE;
    cover cg is { item n: bool; };
    setup() is also { set_config(cover, mode, normal); };
    run() is also {
        emit cg;
        set_config(cover, at_least, 2);
    };
};
