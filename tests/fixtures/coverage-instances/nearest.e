unit base_u {
    event cg;
    !n: bool = FALSE;
    cover cg using per_unit_instance = base_u is { item n: bool; };
    send(v: bool) is { n = v; emit cg; };
};
unit container_u like base_u { child: base_u is instance; };
extend sys {
    top: container_u is instance;
    setup() is also { set_config(cover, mode, normal); };
    run() is also { top.send(FALSE); top.child.send(TRUE); };
};
