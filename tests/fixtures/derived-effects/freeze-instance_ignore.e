extend sys {
    event cg;
    cover cg using per_unit_instance = sys is {
        item n: bool = FALSE using no_collect;
        item m: bool = FALSE using no_collect;
        cross n, m using instance_ignore = FALSE;
    };
    cover cg is also { per_unit_instance = other_u; };
};
unit other_u {};
