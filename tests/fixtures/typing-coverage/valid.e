type mode_t: [active, idle];
struct base_s {
    ev: event;
    count: uint;
};
struct covered_s like base_s {
    mode: mode_t;
    name_collision: uint;
    when active'mode {
        extra: uint;
        cover ev is {
            item qualified: uint = extra + count;
        };
    };
    cover ev is {
        item count: uint;
        item promoted: real = count;
        item unknown: uint = unknown_routine();
        item implicit_unknown: uint;
        item mode: mode_t;
        item comparison: bool = count > 0;
        item name_collision: string = "hello";
        item owner_lookup: uint = name_collision;
        item untyped;
        cross count, promoted;
        transition count;
    };
};
extend covered_s {
    cover ev is also {
        item inherited: uint = count;
    };
};
