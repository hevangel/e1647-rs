unit owner_u {
    event cg;
    flag: bool;
    count(): uint is { return 0; };
    accept(n: uint): bool is { return n > 0; };
    cover cg using per_unit_instance, when = count(), instance_no_collect = inst.count() is {
        item flag: bool using when = accept("bad");
    };
};
