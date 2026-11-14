unit owner_u {
    event cg;
    cover cg using per_unit_instance is {
        item inst: bool using ignore = inst;
    };
};
