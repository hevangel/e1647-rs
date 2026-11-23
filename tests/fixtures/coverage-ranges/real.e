extend sys {
    event cg;
    !n: real = -0.5;
    cover cg is {
        item n: real using ranges = {
            range([-0.5..0.25]);
            range([1.5], "peak", UNDEF, 2);
        };
        transition n;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also {
        emit cg;
        n = 0.25; emit cg;
        n = 1.5; emit cg; emit cg;
    };
};
