struct result_s {
    f(): int is { result = "bad"; };
    f() is also { result = "bad"; };
    static g(): int is { result = "bad"; };
};
