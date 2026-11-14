unit node_u {
    event cg;
    !n: bool = FALSE;
    !back: node_u;
    cover cg using per_unit_instance is { item n: bool; };
    send() is { emit cg; };
};
extend sys {
    !nodes: list of node_u;
    setup() is also { set_config(cover, mode, normal); };
    run() is also {
        var a: node_u = new node_u;
        var b: node_u = new node_u;
        nodes.add(a); nodes.add(b); nodes.add(a);
        a.back = a; b.back = b;
        a.send();
    };
};
