unit channel_u {
    event cg;
    !n: bool = FALSE;
    cover cg using per_unit_instance is { item n: bool; };
    send(v: bool) is { n = v; emit cg; };
};
extend sys {
    a: channel_u is instance;
    b: channel_u is instance;
    c: channel_u is instance;
    setup() is also { set_config(cover, mode, normal); };
    run() is also { a.send(FALSE); b.send(TRUE); };
};
