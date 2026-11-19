extend sys {
    const signed_limit: int = MAX_INT;
    const signed_roll: int = signed_limit + 1;
    const wide_limit: longuint = MAX_LONGUINT;
    const wide_roll: longuint = wide_limit + 1;
    const text_value: string = suffix + "\\n";
    const suffix: string = "prefix";
    cover cg is { item n: bool = FALSE using at_least = wide_limit + 1; };
    setup() is also { set_config(cover, mode, normal); };
    increment(x: int): int is { return x + 1; };
    run() is also {
        emit cg;
        check that signed_roll == MIN_INT;
        check that signed_roll == increment(signed_limit);
        check that wide_roll == 0;
        check that text_value == "prefix\\n";
    };
};
