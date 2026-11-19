unit covered_u {
    event cg;
    count: uint;
    label: string;
    cover cg using per_unit_instance, when = 7, instance_no_collect = label is {
        item n: uint = count using when = count, instance_no_collect = inst.count,
            ignore = n, illegal = "bad", instance_ignore = inst.count, instance_illegal = inst.label;
        item m: uint = count;
        cross n, m using when = "bad", ignore = n + m;
        transition n using illegal = prev_n;
    };
};
