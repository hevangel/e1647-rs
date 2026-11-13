unit mask_u {
    event cg;
    !n: bool = FALSE;
    !hide: bool = FALSE;
    cover cg using per_unit_instance is {
        item n: bool using no_collect;
        item m: bool = FALSE using no_collect;
        cross n, m using name = "p", no_collect, instance_ignore = inst.hide;
        transition p;
    };
    send(value: bool) is { n = value; emit cg; };
};
extend sys {
    a: mask_u is instance;
    setup() is also { set_config(cover, mode, normal); };
    run() is also {
        a.send(FALSE); a.send(TRUE); out(covers.compute_overall_grade(), "\n");
        a.hide = TRUE; out(covers.compute_overall_grade(), "\n");
        a.hide = FALSE; out(covers.compute_overall_grade(), "\n");
    };
};
