extend sys {
    event cg;
    !word: string = "skip";
    cover cg is { item word: string using ignore = word == "skip"; };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; word = "keep"; emit cg; };
};
