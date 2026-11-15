extend sys {
    event cg;
    !n: int = 0;
    cover cg is {
        item n: int using instance_illegal = n == 0;
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also {  };
};
