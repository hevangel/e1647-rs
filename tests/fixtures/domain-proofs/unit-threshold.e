unit proof_u {
    event cg;
    !n: int = 0;
    !limit: int = 0;
    cover cg using per_unit_instance is {
        item n: int using no_collect, ranges = { range([0..1000000000]); };
        item m: int = 0 using no_collect, ranges = { range([0..1000000000]); };
        cross n, m using instance_ignore = n <= inst.limit;
    };
    send(value: int) is { n = value; emit cg; };
};
extend sys {
    a: proof_u is instance;
    b: proof_u is instance;
    setup() is also { set_config(cover, mode, normal); b.limit = 1000000000; };
    run() is also { a.send(0); a.send(1); b.send(0); b.send(1); };
};
