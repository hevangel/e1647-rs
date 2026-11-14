extend sys {
    event cg;
    !n: int = 0;
    !m: int = 0;
    cover cg is {
        item flag: bool = FALSE using no_collect;
        item m: int using no_collect, ranges = { range([0..1000000000]); };
        cross flag, m using ignore = flag or not flag;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; };
};
