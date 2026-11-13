unit rotate_u {
    event cg;
    !n: bool = TRUE;
    !skip: bool = FALSE;
    cover cg using per_unit_instance is {
        item n: bool using no_collect;
        item m: bool = FALSE using no_collect;
        cross n, m using instance_ignore = n == inst.skip;
    };
    send() is { emit cg; };
};
extend sys {
    a: rotate_u is instance;
    setup() is also { set_config(cover, mode, normal); };
    run() is also {
        a.send(); out(covers.compute_overall_grade(), "\n");
        a.skip = TRUE; out(covers.compute_overall_grade(), "\n");
        a.skip = FALSE; out(covers.compute_overall_grade(), "\n");
    };
};
