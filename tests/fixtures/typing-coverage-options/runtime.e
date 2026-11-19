type flag_alias: bool;
unit scope_u {
    event cg;
    !value: bool = FALSE;
    !show: bool = TRUE;
    cover cg using per_unit_instance, when = show, instance_no_collect = not inst.show is {
        item n: flag_alias = value using ignore = n, instance_ignore = not inst.show;
        item m: bool = value using no_collect, when = show;
        cross n, m using ignore = n and m, instance_no_collect = not inst.show;
        transition n using illegal = n and not prev_n;
    };
    send() is { emit cg; emit cg; };
};
extend sys {
    u: scope_u is instance;
    setup() is also { set_config(cover, mode, normal); };
    run() is also { u.send(); out("scope checks passed"); };
};
