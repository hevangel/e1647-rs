struct scopes_s {
    event cg;
    flag: bool;
    count: uint;
    cover cg is {
        item flag: uint = count using when = flag, ignore = flag;
        item count: bool = flag using when = count, ignore = count;
        transition flag using ignore = prev_flag;
        transition count using ignore = prev_count;
        cross flag, count using illegal = flag;
    };
};
