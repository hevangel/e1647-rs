struct probe_s {
    event cg;
    !n: bool = FALSE;
    cover cg using per_unit_instance = hub_u is { item n: bool; };
    send(v: bool) is { n = v; emit cg; };
};
unit leaf_u { p: probe_s; };
unit hub_u { p: probe_s; leaf: leaf_u is instance; };
extend sys {
    a: hub_u is instance;
    b: hub_u is instance;
    setup() is also { set_config(cover, mode, normal); };
    run() is also { a.p.send(FALSE); a.leaf.p.send(TRUE); };
};
