struct api_s {
    convert(n: uint): uint is { return n; };
};
extend api_s {
    convert(arg) is also { return evaluate { value = "bad"; }; };
};
