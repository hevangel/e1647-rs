unit node_u {
    event sample_e;
    !flag: bool = TRUE;
    !hooks: uint = 0;
    on sample_e { hooks = hooks + 1; };
    cover sample_e using per_unit_instance is { item flag: bool; };
};
extend sys {
    a: node_u is instance;
    b: node_u is instance;
    setup() is also { set_config(cover, mode, normal); };
    run() is also {
        var alias := a;
        emit alias.sample_e;
        emit me.a.sample_e;
        emit { a; b; }[1].sample_e;
        check that a.hooks == 2;
        check that b.hooks == 1;
        out("coverage aliases passed");
    };
};
