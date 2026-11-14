extend sys {
    event cg;
    !n: bool = FALSE;
    !m: bool = FALSE;
    !allow: bool = TRUE;
    probe_a(): bool is { out("t"); return TRUE; };
    probe_b(): bool is { out("u"); return TRUE; };
    probe_c(): bool is { out("s"); return TRUE; };
    cover cg is {
        item n: bool using no_collect;
        item m: bool using no_collect;
        transition n using name = "t", no_collect, when = probe_a();
        transition t using no_collect, when = probe_b();
        transition n using name = "s", no_collect, when = probe_c();
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also { emit cg; out("|"); n = TRUE; emit cg; out("|"); n = FALSE; emit cg; out("|\n"); };
};
