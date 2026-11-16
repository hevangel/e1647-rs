type loop_a_t: loop_b_t;
type loop_b_t: loop_a_t;
extend sys {
    const cyclic: loop_a_t = 1;
    const oversized: uint(bits: 128) = 1;
    const outside: uint(bits: 8) = 256;
    const negative: uint(bits: 8) = -1;
    const zero_width: uint(bits: 1 - 1) = 0;
    cover cg is {
        item a: bool = FALSE using at_least = cyclic;
        item b: bool = FALSE using at_least = oversized;
        item c: bool = FALSE using at_least = outside;
        item d: bool = FALSE using at_least = negative;
        item e: bool = FALSE using at_least = zero_width;
    };
};
