extend sys {
    event cg;
    !n: uint = 0;
    cover cg is { item n: uint; };
    run() is also {
        emit cg;
        covers.sample_cg("sys.cg");
        out(covers.compute_overall_grade(), "\n");
    };
};
