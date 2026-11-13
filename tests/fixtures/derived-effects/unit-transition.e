unit prev_u {
    event cg;
    !n: bool = FALSE;
    !drop: bool = TRUE;
    cover cg using per_unit_instance is {
        item n: bool using no_collect;
        transition n using instance_ignore = inst.drop and n == prev_n, illegal = n and not prev_n;
    };
    send(value: bool) is { n = value; emit cg; };
};
extend sys {
    a: prev_u is instance;
    setup() is also { set_config(cover, mode, normal); };
    run() is also { a.send(FALSE); a.send(FALSE); a.send(TRUE); a.send(TRUE); a.send(FALSE); };
};
