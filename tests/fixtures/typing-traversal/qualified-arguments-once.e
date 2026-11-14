type mode_t: [active, idle];
struct child_s {
    mode: mode_t;
    when active'mode { extra: uint; };
    accept(v: uint): uint is { return v; };
};
struct owner_s {
    obj: child_s;
    label: string;
    check() is {
        obj.active'accept(label * 2);
    };
};
