type count_t: int;
extend helper_s {
    add(renamed) is also { return renamed + 2; };
    static twice(renamed) is also { return renamed * 3; };
    static on tick { seen = seen * 10 + 2; };
};
struct helper_s {
    static !seen: int = 0;
    static event tick;
    static on tick { seen = seen * 10 + 1; };
    add(x: count_t): count_t is { return x + 1; };
    static twice(x: int): int is { return x * 2; };
    delayed(x: int): int @sys.any is { sync; return x; };
    short(x: uint(bits: 8)): int is { return x; };
};
extend helper_s {
    short(renamed: byte): int is also { return renamed; };
    delayed(renamed) is also { sync; return renamed + 1; };
};
struct child_s like helper_s { };
extend sys {
    run() is also {
        var helper: child_s = new child_s;
        check that helper.add(5) == 7;
        check that helper_s::twice(5) == 15;
        check that helper.short(7) == 7;
        check that helper.delayed(4) == 5;
        emit helper_s::tick;
        check that helper_s::seen == 12;
        out("method extension contracts passed");
    };
};
