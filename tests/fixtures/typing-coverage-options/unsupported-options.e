extend sys {
    event wrong_group;
    event cg;
    cover wrong_group using typo = TRUE is { item n: bool = FALSE; };
    cover cg is {
        item n: bool = FALSE using unknown_flag;
        item b: bool = TRUE;
        cross n, b using per_instance;
        transition n using ranges = { range([0..1]); };
    };
};
