extend sys {
    event cg;
    !n: int = 0;
    cover cg is {
        item n: int using ranges = { range([0..9]); range([10..19]); },
            ignore = n < 5 or (n >= 10 and n < 15), illegal = n >= 15;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { n = 3; emit cg; n = 7; emit cg; };
};
