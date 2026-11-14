extend sys {
    event cg;
    !n: int = 0;
    !m: int = 0;
    cover cg is {
        item MAX_INT: int using no_collect, ranges = { range([-2..-1]); };
        item m: int using no_collect, ranges = { range([0..1000000000]); };
        cross MAX_INT, m using ignore = MAX_INT < 0;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also {  };
};
