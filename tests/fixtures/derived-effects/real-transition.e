extend sys {
    event cg;
    !word: real = 1.25;
    cover cg is {
        item word: real using no_collect;
        transition word using ignore = word == prev_word;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; emit cg; word = 2.5; emit cg; };
};
