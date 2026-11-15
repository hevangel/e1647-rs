extend sys {
    event cg;
    !n: uint = 0;
    cover cg is { item n: uint; };
    setup() is also { set_config(cover, mode, TRUE, max_int_buckets, 0); };
    run() is also { emit cg; };
};
