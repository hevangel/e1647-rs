struct totals_s {
    sum(value: int): int is { return value + 30; };
};
extend sys {
    !calls: uint = 0;
    !totals: totals_s;

    get_totals(): totals_s is {
        calls = calls + 1;
        return totals;
    };
    run() is also {
        var values: list of real = {2.0; 3.5};
        out(values.product(), " ", values.product(it + 1.0), "\n");
        out(values.min_value(it + 1.0), " ", values.max_value(it), "\n");
        out(get_totals().sum(12), " ", calls, "\n");
    };
};
