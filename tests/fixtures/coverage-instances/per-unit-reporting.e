unit channel_u {
    event cg;
    !state: bool = FALSE;
    !accepted: bool = FALSE;
    !collect_transition: bool = TRUE;
    cover cg using per_unit_instance is {
        item state: bool using no_collect;
        item accepted: bool;
        cross state, accepted using name = "paired";
        transition state using name = "changes", when = collect_transition;
    };
    send(s: bool, v: bool, enabled: bool) is {
        state = s;
        accepted = v;
        collect_transition = enabled;
        emit cg;
    };
};
extend sys {
    a: channel_u is instance;
    b: channel_u is instance;
    setup() is also { set_config(cover, mode, normal); };
    run() is also {
        a.send(FALSE, FALSE, TRUE);
        b.send(TRUE, FALSE, TRUE);
        a.send(TRUE, FALSE, FALSE);
        b.send(TRUE, FALSE, TRUE);
        a.send(FALSE, TRUE, TRUE);
        b.send(FALSE, FALSE, TRUE);
    };
};
