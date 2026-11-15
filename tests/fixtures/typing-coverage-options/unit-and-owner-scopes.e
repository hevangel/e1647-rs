unit scope_u {
    count: uint;
    flag: bool;
};
struct owner_s {
    event cg;
    count: bool;
    inst: bool;
    cover cg using per_unit_instance = scope_u, when = inst, instance_no_collect = inst.count is {
        item count: bool using when = inst, instance_no_collect = inst.flag,
            instance_ignore = inst.count;
    };
};
