extend sys {
    event cg;
    !n: int = 0;
    cover cg is {
        item n: int using ranges = { range([0..100000]); }, ignore = n % 2 == 0;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; };
};
