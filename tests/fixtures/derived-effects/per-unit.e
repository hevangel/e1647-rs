unit filter_u {
    event cg;
    !n: bool = FALSE;
    !m: bool = FALSE;
    !drop: bool = FALSE;
    !bad: bool = FALSE;
    cover cg using per_unit_instance is {
        item n: bool using no_collect;
        item m: bool using no_collect;
        cross n, m using ignore = FALSE, instance_ignore = inst.drop and n == m,
            illegal = FALSE, instance_illegal = inst.bad and n;
    };
    send(x: bool, y: bool) is { n = x; m = y; emit cg; };
};
extend sys {
    a: filter_u is instance;
    b: filter_u is instance;
    setup() is also { set_config(cover, mode, normal); a.drop = TRUE; b.bad = TRUE; };
    run() is also {
        a.send(FALSE, FALSE); a.send(TRUE, FALSE); a.send(TRUE, TRUE); a.send(FALSE, TRUE);
        b.send(FALSE, FALSE); b.send(TRUE, FALSE); b.send(TRUE, TRUE); b.send(FALSE, TRUE);
    };
};
