type ranged_t: uint [0..7];
struct unresolved_s {
    width(x: uint(bits: UNKNOWN_WIDTH)) is { };
    ranged(x: ranged_t) is { };
    no_prototype(x: string) is also { };
};
extend unresolved_s {
    width(x: uint(bits: 8)) is also { };
    ranged(x: uint) is also { };
};
