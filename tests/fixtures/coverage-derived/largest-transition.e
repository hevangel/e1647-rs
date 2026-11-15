extend sys {
    event cg;
    cover cg is {
        item n: uint = 0 using no_collect;
        transition n;
    };
    setup() is also { set_config(cover, mode, normal, max_int_buckets, 65536); };
};
