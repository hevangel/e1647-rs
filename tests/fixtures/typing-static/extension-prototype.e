extend api_s {
    static accept(value) is also { return "bad"; };
};
struct api_s {
    static accept(n: uint): uint is { return n; };
};
struct owner_s {
    check() is { api_s::accept("bad"); };
};
