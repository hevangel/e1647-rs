// Independently authored example: IEEE 1647-2019, 14.5.
extend sys {
    event sampled;
    !n: int = 0;
    cover sampled is {
        item n: int using no_collect, ranges = { range([0..1], "", 1); };
        transition n using ignore = n == prev_n;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit sampled; emit sampled; n = 1; emit sampled; n = 0; emit sampled; };
};
