extend sys {
    event cg;
    cover cg using at_least = -1 is { item n: bool = TRUE; };
};
