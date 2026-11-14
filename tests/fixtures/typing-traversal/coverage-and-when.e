type mode_t: [active, idle];
struct owner_s {
    event cg;
    mode: mode_t;
    label: string;
    count: uint;
    when active'mode {
        cover cg is {
            item n: bool = count in [(label * 2)..3] using ignore = n in [("bad" * 2)..3];
        };
    };
};
