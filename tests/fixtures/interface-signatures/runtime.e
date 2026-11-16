type count_t: int;
interface compute_i { twice(x: count_t): count_t; };
interface sized_i { width(x: uint(bits: 8)): int; };
interface byte_i { short_width(x: uint(bits: 8)): int; };
struct base_s {
    twice(renamed: int): int is { return renamed * 2; };
};
struct child_s like base_s implementing compute_i { };
extend child_s implementing sized_i, byte_i { };
extend child_s {
    width(renamed: uint(bits: 4 + 4)): int is { return renamed; };
    short_width(renamed: byte): int is { return renamed; };
};
extend sys {
    run() is also {
        var target: child_s = new child_s;
        var handle: compute_i = target;
        check that handle.twice(7) == 14;
        check that target.width(3) == 3;
        check that target.short_width(5) == 5;
        out("interface signatures passed");
    };
};
