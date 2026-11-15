unit node_u {
    event cg;
    !n: uint = 0;
    cover cg using per_unit_instance is { item n: uint using ranges = { range([0..1], "pa" + "ir", UNDEF, 2); }; };
    send() is { emit cg; };
};
extend sys {
    a: node_u is instance;
    b: node_u is instance;
    setup() is also { set_config(cover, mode, normal, at_least, 3); };
    run() is also { a.send(); a.send(); b.send(); };
};
