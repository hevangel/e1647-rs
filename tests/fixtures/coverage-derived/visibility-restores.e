unit derived_u {
    event cg;
    !hide: bool = FALSE;
    cover cg using per_unit_instance is {
        item n: bool = FALSE using no_collect;
        item flag: bool = FALSE using no_collect;
        cross n, flag using instance_no_collect = inst.hide;
    };
    send() is { emit cg; };
};
extend sys {
    a: derived_u is instance;
    setup() is also { set_config(cover, mode, normal); a.hide = TRUE; };
    run() is also {
        a.send(); out(covers.compute_overall_grade(), "\n");
        a.hide = FALSE; out(covers.compute_overall_grade(), "\n");
    };
};
