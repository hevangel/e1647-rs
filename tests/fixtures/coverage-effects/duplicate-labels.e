extend sys {
    event cg;
    !n: int = 0;
    cover cg is {
        item n: int using ranges = { range([0..0], "same"); range([1..1], "same"); range([2..2], "other"); range([3..3], "other"); }, ignore = n == 2, per_instance;
        item flag: bool = FALSE;
        cross n, flag;
        transition n;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { for i from 0 to 3 { n = i; emit cg; }; };
};
