extend sys {
    event cg;
    !n: int = 0;
    cover cg is {
        item n: int using illegal = n.to_string() == "0";
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also {  };
};
