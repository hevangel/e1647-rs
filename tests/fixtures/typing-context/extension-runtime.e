extend api_s {
    static convert(arg) is also {
        arg = evaluate (result) { result = arg; };
        return evaluate { value = arg; };
    };
};
struct api_s {
    static convert(n: uint): uint is { return n; };
    ordinary(n: uint): uint is { return n; };
};
extend api_s {
    ordinary(arg) is also {
        arg = evaluate (result) { result = arg; };
        return evaluate { value = arg; };
    };
};
extend sys {
    api: api_s;
    run() is also {
        check that api_s::convert(evaluate { value = 5; }) == 5;
        check that api.ordinary(evaluate { value = 6; }) == 6;
        out("extension contexts passed");
    };
};
