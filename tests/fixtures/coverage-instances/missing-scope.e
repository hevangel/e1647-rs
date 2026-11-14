extend sys {
    event cg;
    cover cg using instance_no_collect = TRUE is { item n: bool = TRUE; };
};
