extend sys {
    event cg;
    cover cg is {
        item n0: uint = 0 using no_collect;
        item n1: uint = 0 using no_collect;
        item n2: uint = 0 using no_collect;
        item n3: uint = 0 using no_collect;
        item n4: uint = 0 using no_collect;
        item n5: uint = 0 using no_collect;
        item n6: uint = 0 using no_collect;
        item n7: uint = 0 using no_collect;
        item n8: uint = 0 using no_collect;
        item n9: uint = 0 using no_collect;
        item n10: uint = 0 using no_collect;
        item n11: uint = 0 using no_collect;
        item n12: uint = 0 using no_collect;
        item n13: uint = 0 using no_collect;
        item n14: uint = 0 using no_collect;
        item n15: uint = 0 using no_collect;
        cross n0, n1, n2, n3, n4, n5, n6, n7, n8, n9, n10, n11, n12, n13, n14, n15;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; };
};
