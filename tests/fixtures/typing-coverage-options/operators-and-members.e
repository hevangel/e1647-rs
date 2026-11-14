unit scope_u {
    event cg;
    label: string;
    cover cg using per_unit_instance, when = label * 2 > 0, instance_no_collect = inst.missing is {
        item n: uint using ignore = n + "bad" > 0, instance_ignore = inst.missing;
    };
};
