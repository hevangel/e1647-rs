extend sys {
    event sampled;
    !value: uint = 0;
    cover sampled is {
        item value: uint using ranges = {
            range([0..9], "", 3, 2);
            range([10..11], "optional", UNDEF, 0);
        };
    };
    setup() is also { set_config(cover, mode, normal, at_least, 1); };
    run() is also {
        for i from 0 to 9 { value = i; emit sampled; };
    };
};
