unit graph_u {
    event cg;
    !n: int = 0;
    !skip: int = -1;
    cover cg using per_unit_instance is {
        item n: int using no_collect, ranges = { range([0..1], "", 1); }, instance_ignore = inst.skip == 0;
        item m: bool = FALSE using no_collect;
        cross n, m using name = "p", no_collect;
        transition p;
    };
    send(value: int) is { n = value; emit cg; };
};
extend sys {
    a: graph_u is instance;
    setup() is also { set_config(cover, mode, normal); };
    run() is also { a.send(0); a.send(1); a.skip = 0; };
};
