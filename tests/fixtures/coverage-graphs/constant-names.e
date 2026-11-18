extend sys {
    !n: bool = FALSE;
    cover cg is {
        item n: bool using no_collect;
        item m: bool = FALSE using no_collect;
        cross n, m using name = ("pa" + "ir"), no_collect;
        cross pair, n using name = ("quoted\"" + "\\Ω");
        transition n using name = "walk" + ("ed");
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; n = TRUE; emit cg; };
};
