extend sys {
    !word: string = "a";
    cover cg is {
        item word: string using no_collect;
        item flag: bool = FALSE using no_collect;
        cross word, flag using name = "wordsΩ", at_least = 18446744073709551615, weight = 18446744073709551615;
        transition word using at_least = 0, weight = 0;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; word = "b"; emit cg; };
};
