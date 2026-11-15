extend sys {
    event cg;
    !n: int = 0;
    cover cg is {
        item n: int using ranges = { range([MIN_INT..MAX_INT]); }, ignore = n < MIN_INT or MAX_INT < n;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; };
};
