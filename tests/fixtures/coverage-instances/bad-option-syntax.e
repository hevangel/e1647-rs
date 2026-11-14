extend sys {
    event cg;
    cover cg using when = (TRUE and) is { item n: bool = TRUE; };
};
