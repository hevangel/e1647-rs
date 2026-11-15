extend sys {
    event cg;
    event global_cg;
    !n: uint = 0;
    cover cg using at_least = 3 is {
        item n: uint using at_least = 2 + 2, ranges = {
            range([0..5], "", 2, 5);
            range([6..7], "free", UNDEF, 0);
            range([8], "named");
            range([9]);
        };
        item fallback: uint = 10 using ranges = { range([10..11], "fallback"); };
    };
    cover global_cg is { item flag: bool = TRUE; };
    setup() is also { set_config(cover, mode, normal, at_least, 2); };
    run() is also {
        for i from 0 to 9 { n = i; emit cg; };
        n = 0;
        for i from 0 to 3 { emit cg; };
        emit global_cg;
    };
};
