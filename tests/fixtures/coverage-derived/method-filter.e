extend sys {
    event cg;
    !n: bool = FALSE;
    !allow: bool = FALSE;
    permitted(): bool is { return allow; };
    cover cg is {
        item n: bool using no_collect;
        item flag: bool = FALSE using no_collect;
        cross n, flag using when = permitted(), weight = 3;
        transition n using when = FALSE, weight = 1;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { allow = TRUE; emit cg; };
};
