struct base_s {
    f(x: int): int is { return x; };
    g(x: int) is { };
    untyped(x) is { };
    void_result() is { };
    static s(x: int): int is { return x; };
};
extend base_s {
    f(x: string): int is also { return 0; };
    g() is first { };
    untyped(x: int) is only { };
    void_result(): int is also { return 0; };
    static s(x: string): int is only { return 0; };
    static f(x: int): int is also { return x; };
    s(x: int): int is also { return x; };
};
struct inherited_s like base_s { };
