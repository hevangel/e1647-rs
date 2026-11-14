// Independently authored example: IEEE 1647-2019, 14.4.
extend sys {
    event sampled;
    !n: int = 0;
    !m: int = 0;
    cover sampled is {
        item n: int using no_collect, ranges = { range([0..1000000000]); };
        item m: int using no_collect, ranges = { range([0..1000000000]); };
        cross n, m using ignore = n <= m;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit sampled; n = 1; emit sampled; m = 1; emit sampled; n = 2; emit sampled; };
};
