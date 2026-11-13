unit filtered_u {
    event cg;
    !n: int = 0;
    !skip: int = 0;
    !bad: int = 1;
    cover cg using per_unit_instance is {
        item n: int using ranges = { range([0..2], "", 1); },
            ignore = n == 0, instance_ignore = n == inst.skip, illegal = n == 2, instance_illegal = n == inst.bad;
    };
    send(v: int) is { n = v; emit cg; };
};
extend sys {
    a: filtered_u is instance;
    b: filtered_u is instance;
    setup() is also { set_config(cover, mode, normal); b.skip = 2; b.bad = 0; };
    run() is also { a.send(0); a.send(1); a.send(2); b.send(0); b.send(1); b.send(2); };
};

