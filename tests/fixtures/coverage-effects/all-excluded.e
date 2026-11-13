extend sys {
    event cg;
    !n: int = 0;
    cover cg is {
        item n: int using ranges = { range([0..3], "", 1); }, ignore = TRUE, per_instance;
        item flag: bool = FALSE;
        cross n, flag;
        transition n;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { for i from 0 to 3 { n = i; emit cg; }; };
};
