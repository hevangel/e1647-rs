type observed_t: int;
extend sys {
    event cg;
    !counter: int = 0;
    cover cg is also { transition observed; };
    cover cg is {
        item observed: observed_t = counter using no_collect, ranges = { range([0..1], "", 1); }, ignore = observed == 0;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; counter = 1; emit cg; emit cg; };
};
