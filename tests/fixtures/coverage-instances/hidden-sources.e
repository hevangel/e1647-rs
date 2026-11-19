extend sys {
    event cg;
    !a: bool = FALSE;
    !b: bool = FALSE;
    cover cg is {
        item a: bool using no_collect;
        item b: bool;
        cross a, b;
        transition a;
        transition a using name = "other_transition";
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; a = TRUE; b = TRUE; emit cg; };
};
