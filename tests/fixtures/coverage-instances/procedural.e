unit node_u {
    event cg;
    !n: bool = FALSE;
    cover cg using per_unit_instance, when = n is { item n: bool; };
};
extend sys {
    a: node_u is instance;
    b: node_u is instance;
    setup() is also { set_config(cover, mode, normal); };
    run() is also { a.n = TRUE; covers.sample_cg("node_u.cg"); };
};
