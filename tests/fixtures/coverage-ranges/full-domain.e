extend sys {
    event cg;
    !n: int = 0;
    cover cg is { item n: int using ranges = { range([-170141183460469231731687303715884105727 - 1..170141183460469231731687303715884105727], "full", UNDEF, 2); }; };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; };
};
