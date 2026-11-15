extend sys {
    event cg;
    !n: int = 0;
    cover cg is { item n: int using ranges = {
        range([-5..(2 + 2)], "", 1 + 2, UNDEF);
        range([-3..-1], "overlap", UNDEF, 1);
    }; };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { for i from -5 to 4 { n = i; emit cg; }; };
};
