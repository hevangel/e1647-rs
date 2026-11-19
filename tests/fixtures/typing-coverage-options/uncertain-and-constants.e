define MAX_INT TRUE;
struct scopes_s {
    event cg;
    flag: bool;
    cover cg using when = unresolved() is {
        item MAX_INT: uint using ignore = MAX_INT;
        item MIN_INT: bool using illegal = MIN_INT > 0;
        item n: uint using when = unknown_value;
        cross MAX_INT, n using ignore = MAX_INT;
    };
};
