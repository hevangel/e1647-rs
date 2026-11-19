extend sys {
    event cg;
    !word: string = "a";
    cover cg is {
        item word: string using no_collect;
        transition word using ignore = word == prev_word;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; emit cg; word = "b"; emit cg; };
};
