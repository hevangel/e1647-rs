type bit_alias: bool;
extend base_u {
    cover cg is also {
        transition flag using ignore = prev_flag;
    };
};
unit base_u {
    event cg;
    count: uint;
    flag: bit_alias;
    cover cg using per_unit_instance is {
        item flag: bit_alias using ignore = flag, instance_ignore = inst.flag;
        item count: uint using ignore = count;
    };
};
unit derived_u like base_u {
    cover cg is also {
        cross count, flag using illegal = count;
    };
};
