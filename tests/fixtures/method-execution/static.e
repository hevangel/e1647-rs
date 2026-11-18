struct api_s {
    static !trace: string = "";
    static !result: string = "field";
    static f(x: int): int is { trace = append(trace, "C"); result = x; x = 99; };
    static f(renamed) is also { trace = append(trace, "A"); result = result + renamed; };
    static f(renamed) is first { trace = append(trace, "F"); renamed = renamed + 1; result = 999; };
    static replaced(): int is { return -1; };
    static replaced() is first { return -2; };
    static replaced() is only { result = 7; };
    static replaced() is also { result = result + 1; };
};
extend sys {
    run() is also {
        check that api_s::f(2) == 6;
        check that api_s::trace == "FCA";
        check that api_s::result == "field";
        check that api_s::replaced() == 8;
        out("static wrappers passed\n");
    };
};
