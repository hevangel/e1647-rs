type count_t: uint(bits: 8);
type count_alias_t: count_t;
type text_t: string;
type signed_t: int;
type long_t: longuint;
struct limits_s {
    static const limit_value: long_t = MAX_LONGUINT;
    static const wrapped_value: long_t = limit_value + 1;
};
extend sys {
    const unbounded: int(bits: *) = 4294967295;
    const unbounded_sum: int = unbounded + 1;
    const widest: uint(bits: 127) = 170141183460469231731687303715884105727;
    const signed_small: int(bits: 8) = -128;
    const width_value: int = 8;
    const sized: uint(bits: width_value) = 1;
    const lo: count_alias_t = 0;
    const hi: count_alias_t = sized;
    const hit_goal: count_alias_t = 2;
    const label_value: text_t = "aliased";
    const signed_limit: signed_t = MAX_INT;
    const signed_roll: signed_t = signed_limit + 1;
    const wide_limit: uint(bits: 40) = 1;
    const wide_value: int = wide_limit + 4294967295;
    const zero_goal: long_t = limits_s::limit_value + 1;
    !value: int = 0;
    cover cg is {
        item n: uint = value using ranges = { range([lo..hi], "", sized, hit_goal); };
        item m: bool = FALSE using no_collect;
        cross n, m using name = label_value;
    };
    cover ungraded is { item n: bool = FALSE using at_least = zero_goal; };
    increment(x: signed_t): signed_t is { return x + 1; };
    grow(x: uint(bits: 40)): int is { return x + 4294967295; };
    setup() is also { set_config(cover, mode, normal); };
    run() is also {
        check that signed_small == -128;
        check that signed_roll == MIN_INT;
        check that increment(signed_limit) == MIN_INT;
        check that wide_value == 4294967296;
        check that grow(wide_limit) == wide_value;
        check that limits_s::wrapped_value == 0;
        check that limits_s::limit_value + 1 == 0;
        check that zero_goal == 0;
        out(signed_roll, "\n", wide_value, "\n", zero_goal, "\n", widest, "\n", unbounded_sum, "\n");
        emit ungraded;
        emit cg; value = 1; emit cg; value = 0; emit cg; value = 1; emit cg;
    };
};
