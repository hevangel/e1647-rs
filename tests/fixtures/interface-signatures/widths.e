interface widths_i {
    defaulted(x: uint(bits: 8));
    long_mode(x: int(bits: 64));
    short_mode(x: uint(bits: 4));
    result(): int;
};
struct widths_s implementing widths_i {
    defaulted(x: uint) is { };
    long_mode(x: longint) is { };
    short_mode(x: bit) is { };
    result() is { };
};
