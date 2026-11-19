extend sys {
    !value: uint = 0;
    cover cg is {
        item n: uint = value using ranges = { range([0..0], "quote\"" + "\\Ω"); range([1..1], "\\" + "n"); };
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; value = 1; emit cg; };
};
