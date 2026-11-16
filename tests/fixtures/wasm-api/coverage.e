extend sys {
    !flag: bool = FALSE;
    cover cg is { item flag: bool; };
    setup() is also { set_config(cover, mode, normal); };
    run() is also {
        emit cg;
        flag = TRUE;
        emit cg;
    };
};
