unit sample_u {
    event cg;
    !blocked: bool = FALSE;
    !hide_n: bool = FALSE;
    !n: bool = FALSE;
    cover cg using per_unit_instance, instance_no_collect = inst.blocked is {
        item n: bool using instance_no_collect = inst.hide_n;
        item other: bool = TRUE;
        cross n, other;
    };
    send() is { emit cg; };
};
extend sys {
    a: sample_u is instance;
    b: sample_u is instance;
    setup() is also { set_config(cover, mode, normal); };
    run() is also { a.hide_n = TRUE; b.blocked = TRUE; a.send(); b.send(); };
};
