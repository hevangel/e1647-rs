struct owner_s {
    xs: list of uint;
    check() is {
        var n: uint = xs.count(evaluate { value = 1; });
        var selected: list of uint = xs.all(evaluate { value = TRUE; });
    };
};
