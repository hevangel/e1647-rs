unit history_u {
    event cg;
    !n: int = 0;
    !skip: int = -1;
    cover cg using per_unit_instance is {
        item n: int using ranges = { range([0..1], "", 1); }, instance_ignore = n == inst.skip;
        item flag: bool = FALSE;
        cross n, flag;
        transition n;
    };
    send(v: int) is { n = v; emit cg; };
};
extend sys {
    a: history_u is instance;
    setup() is also { set_config(cover, mode, normal); };
    run() is also {
        a.send(0); a.send(1); a.skip = 0;
        out(covers.compute_overall_grade(), "\n");
        a.send(0);
    };
};
