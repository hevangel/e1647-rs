unit owner_u {
    event cg;
    count: uint;
    labels: list of string;
    cover cg using per_unit_instance, instance_no_collect = inst.labels[0] is {
        item count: uint using when = count[0], instance_ignore = inst.labels[0];
    };
};
