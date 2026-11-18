struct config_s {
    !seed: uint = evaluate { value = 2; };
    static !limit: uint = evaluate { value = 3; };
};
extend sys {
    cfg: config_s;
    run() is also {
        check that cfg.seed == 2;
        check that config_s::limit == 3;
        out("field contexts passed");
    };
};
