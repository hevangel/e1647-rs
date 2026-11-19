extend sys {
    event cg;
    !n: bool = FALSE;
    !m: bool = FALSE;
    !allow: bool = TRUE;
    cover cg is {
        item n: bool using no_collect;
        item m: bool using no_collect;
        transition n using name = "t0", no_collect;
        transition t0 using name = "t1", no_collect;
        transition t1 using name = "t2", no_collect;
        transition t2 using name = "t3", no_collect;
        transition t3 using name = "t4", no_collect;
        transition t4 using name = "t5", no_collect;
        transition t5 using name = "t6", no_collect;
        transition t6 using name = "t7", no_collect;
        transition t7 using name = "t8", no_collect;
        transition t8 using name = "t9", no_collect;
        transition t9 using name = "t10", no_collect;
        transition t10 using name = "t11", no_collect;
        transition t11 using name = "t12", no_collect;
        transition t12 using name = "t13", no_collect;
        transition t13 using name = "t14", no_collect;
        transition t14 using name = "t15", no_collect;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also {  };
};
