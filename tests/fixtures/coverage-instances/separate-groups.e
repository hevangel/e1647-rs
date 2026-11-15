extend sys {
    event first_cg;
    event second_cg;
    cover first_cg is { item value: uint = 2; };
    cover second_cg is { item value: uint = 3; };
    setup() is also { set_config(cover, mode, normal, max_int_buckets, 4); };
    run() is also { emit first_cg; emit second_cg; };
};
