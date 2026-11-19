extend sys {
    event cg;
    !n: uint = 0;
    cover cg is {
        item n: uint using ranges = { range([0..3]); };
        item flag: bool = FALSE;
        cross n, flag;
        transition n;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { for i from 0 to 2 { n = i; emit cg; }; };
};
