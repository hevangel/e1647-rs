struct counted_s implementing numeric_i of (counted_s, TRUE, 32) {
    !raw: int;
    set_initial_value() is { raw = 0; };
    numeric_assign(other: counted_s) is { raw = other.raw; };
    numeric_from_int(n: int) is { raw = n; };
    numeric_compare(other: counted_s): int is {
        sys.compares = sys.compares + 1;
        if raw == other.raw { return 0; };
        return -1;
    };
    numeric_to_string(radix: numeric_format_kind): string is {
        sys.formats = sys.formats + 1;
        return append(raw);
    };
};
numeric_type counted_t: counted_s;
extend sys {
    !compares: uint = 0;
    !formats: uint = 0;
    later(n: uint): uint is { sync; return n; };
    run() is also {
        var a: counted_t = 1;
        var b: counted_t = 2;
        case a { b: { out("wrong\n"); }; later(1): { out("matched\n"); }; };
        print a, later(3);
        out("formats ", formats, " compares ", compares, "\n");
    };
};
