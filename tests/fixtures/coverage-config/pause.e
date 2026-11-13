extend sys {
    event cg;
    !n: uint = 0;
    !evaluations: uint = 0;
    bucket_count(): uint is { evaluations = evaluations + 1; return 4; };
    cover cg is { item n: uint; };
    setup() is also { set_config(cover, mode, normal, max_int_buckets, bucket_count()); };
    run() is also {
        emit cg;
        set_config(cover, mode, off);
        n = 1; emit cg;
        set_config(cover, mode, count_only);
        n = 2; covers.sample_cg("sys.cg");
        set_config(cover, mode, FALSE);
        n = 3; emit cg;
        out(evaluations, " ", covers.compute_overall_grade(), "\n");
    };
};
