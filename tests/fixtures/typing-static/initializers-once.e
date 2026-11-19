type mode_t: [active, idle];
struct base_s {
    mode: mode_t;
    static !label: string = 3;
    when active'mode { extra: uint; };
};
struct child_s like base_s { };
