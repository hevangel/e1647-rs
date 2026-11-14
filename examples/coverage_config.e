extend sys {
    event transaction;
    !value: uint = 0;
    cover transaction is { item value: uint; };
    setup() is also {
        set_config(cover, mode, normal, max_int_buckets, 4);
    };
    run() is also {
        for i from 0 to 3 {
            value = i;
            emit transaction;
        };
        out("grade=", covers.compute_overall_grade(), "\n");
    };
};
