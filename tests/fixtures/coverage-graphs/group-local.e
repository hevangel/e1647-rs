extend sys {
    event first_cg;
    event second_cg;
    cover first_cg is { item n: bool = FALSE; };
    cover second_cg is { item m: bool = FALSE; cross n, m; };
};
