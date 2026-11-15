extend sys {
    event cg;
    cover cg using per_unit_instance = sys, instance_no_collect = TRUE is { item n: bool = TRUE; };
    cover cg is also { per_unit_instance = other_u; };
};
unit other_u {};
