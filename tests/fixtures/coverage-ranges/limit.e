extend sys {
    event cg;
    cover cg is { item n: uint = 0 using ranges = { range([0..65535], "", 1); }; };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; };
};
