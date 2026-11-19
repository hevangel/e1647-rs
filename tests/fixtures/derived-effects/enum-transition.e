type state_t: [IDLE, BUSY];
extend sys {
    event cg;
    !word: state_t = IDLE;
    cover cg is {
        item word: state_t using no_collect;
        transition word using ignore = word == prev_word;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; emit cg; word = BUSY; emit cg; };
};
