unit derived_u {
    event cg;
    !hide: bool = FALSE;
    cover cg using per_unit_instance is {
        item n: bool = FALSE using no_collect;
        item flag: bool = FALSE using no_collect;
        cross n, flag using when = not hide;
        transition n using when = not hide;
    };
    send() is { emit cg; };
};
extend sys {
    a: derived_u is instance;
    b: derived_u is instance;
    setup() is also { set_config(cover, mode, normal); a.hide = TRUE; };
    run() is also { a.send(); b.send(); b.send(); };
};
