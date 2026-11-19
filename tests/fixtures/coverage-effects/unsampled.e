extend sys {
    event cg;
    !n: int = 0;
    cover cg is {
        item n: int using ranges = { range([0..3], "", 1); }, ignore = n < 3;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also {  };
};
