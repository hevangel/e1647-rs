struct marker_s {};
extend sys {
    event cg;
    !p: marker_s;
    accept(a: bool, b: bool): bool is { return a and not b; };
    cover cg using when = accept(TRUE, FALSE) and p is a marker_s is { item n: bool = TRUE; };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { p = new marker_s; emit cg; };
};
