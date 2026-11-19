struct orphan_s {
    event cg;
    cover cg using per_unit_instance = sys is { item n: bool = TRUE; };
    send() is { emit cg; };
};
extend sys {
    setup() is also { set_config(cover, mode, normal); };
    run() is also { var p: orphan_s = new orphan_s; p.send(); };
};
