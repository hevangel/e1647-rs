// Independently authored example: IEEE 1647-2019, 14.3 and 29.9.
extend sys {
    event sampled;
    !value: int = 0;
    cover sampled is {
        item value: int using ranges = { range([0..5], "", 1); },
            ignore = value == 0, illegal = value == 5;
    };
    setup() is also {
        set_config(cover, mode, normal, check_illegal_immediately, TRUE);
    };
    run() is also {
        for i from 0 to 5 { value = i; emit sampled; };
    };
};
