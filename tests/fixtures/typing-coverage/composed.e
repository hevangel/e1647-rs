type mode_t: [active, idle];
struct base_s {
    ev: event;
    count: uint;
};
struct covered_s like base_s {
    mode: mode_t;
    when active'mode {
        label: string;
        cover ev is {
            item bad: uint = label;
        };
    };
};
extend covered_s {
    cover ev is also {
        item extra: string = count;
    };
};
