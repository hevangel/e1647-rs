type small_t: uint(bits: 1 + 1);
type count_t: small_t;
type bool_t: bool;
type flag_t: bool_t;
extend sys {
    event cg;
    !count: count_t = 0;
    !flag: flag_t = FALSE;
    cover cg using radix = HEX is {
        item count;
        item flag: flag_t;
    };
    setup() is also { set_config(cover, mode, normal, max_int_buckets, 10); };
    run() is also {
        emit cg;
        count = 3;
        flag = TRUE;
        emit cg;
    };
};
