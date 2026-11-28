struct fallback_sample_s {
    a: uint(bits: 3);
    b: uint(bits: 2);

    keep b in [1 .. 3];
    keep a + b == 3 && a / b == 2;
};
struct unsigned_sample_s {
    code: uint(bits: 3);

    // The ordered comparison interprets -1 as a 32-bit unsigned value.
    keep code < -1 => code == 2;
};
extend sys {
    run() is also {
        var sample: fallback_sample_s;
        var unsigned_sample: unsigned_sample_s;
        for i from 1 to 4 {
            gen sample;
            out(sample.a, " ", sample.b, "\n");
        };
        gen unsigned_sample;
        out(unsigned_sample.code, "\n");
    };
};
