struct api_s {
    static accept(n: uint): uint is { return n; };
};
extend api_s {
    static accept(value) is also { var a: bool = value; };
};
