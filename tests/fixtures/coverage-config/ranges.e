extend sys {
    event cg;
    !n: uint = 0;
    cover cg is {
        item n: uint using ranges = { range([0..1], "low"); range([2..3], "high"); };
        item flag: bool = TRUE;
    };
    setup() is also { set_config(cover, max_int_buckets, 1, mode, normal); };
    run() is also { emit cg; n = 3; emit cg; };
};
