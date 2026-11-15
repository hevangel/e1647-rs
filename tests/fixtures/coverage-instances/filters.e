extend sys {
    event cg;
    !enabled: bool = TRUE;
    !take: bool = TRUE;
    !a: bool = FALSE;
    !b: bool = FALSE;
    cover cg using when = enabled is {
        item a: bool using when = take;
        item b: bool;
        cross a, b;
        transition a;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also {
        var enabled: bool = FALSE;
        emit cg;
        take = FALSE; a = TRUE; b = TRUE; emit cg;
        take = TRUE; emit cg;
        me.enabled = FALSE; emit cg;
    };
};
