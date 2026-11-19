extend sys {
    event cg;
    !n: int = 0;
    cover cg is {
        item n: int using ranges = { range([0..1], "", 1); }, illegal = n == 0;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; stop_run(); };
};
