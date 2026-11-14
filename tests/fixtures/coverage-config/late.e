extend sys {
    event cg;
    !n: uint = 0;
    cover cg is { item n: uint; };
    setup() is also { set_config(cover, mode, normal, max_int_buckets, 2); };
    run() is also {
        emit cg;
        set_config(cover, mode, off);
        set_config(cover, max_int_buckets, 3, mode, normal);
    };
};
