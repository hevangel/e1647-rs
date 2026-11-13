extend sys {
    event cg;
    !n: int = 0;
    cover cg is {
        item n: int using ranges = { range([-3..3], "", 1); }, ignore = n % 2 == 0;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { for i from -3 to 3 { n = i; emit cg; }; };
};
