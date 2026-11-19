type mode_t: [active, idle];
struct base_s {
    mode: mode_t;
    when active'mode { extra: uint; };
    static accept(): uint is { return "bad"; };
};
struct child_s like base_s { };
