type ranged_t: uint [0..7];
interface flexible_i {
    width(x: uint(bits: UNKNOWN_WIDTH));
    ranged(x: ranged_t);
};
struct flexible_s implementing flexible_i {
    width(x: uint(bits: 8)) is { };
    ranged(x: uint) is { };
};
struct extension_only_s implementing flexible_i { };
extend extension_only_s { width(x) is also { }; };
