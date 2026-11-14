type mode_t: [active, idle];
struct owner_s {
    event cg;
    mode: mode_t;
    flag: bool;
    when active'mode {
        extra: bool;
        cover cg using when = extra is {
            item flag: uint = 0 using when = flag, ignore = flag > 0;
        };
    };
    get_flag(): bool is { return flag; };
};
struct other_s {
    event cg;
    flag: string;
    cover cg using when = flag == "yes" is {
        item flag: bool = TRUE using ignore = flag;
    };
};
