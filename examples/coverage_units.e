unit channel_u {
    event sample_e;
    !accepted: bool = FALSE;
    cover sample_e using per_unit_instance is { item accepted: bool; };
    record(value: bool) is { accepted = value; emit sample_e; };
};
extend sys {
    a: channel_u is instance;
    b: channel_u is instance;
    setup() is also { set_config(cover, mode, normal); };
    run() is also { a.record(TRUE); b.record(FALSE); };
};
