struct sample_s {
    code: uint(bits: 8);
    keep code in [12 .. 19];
};
extend sys {
    run() is also {
        var sample: sample_s;
        gen sample;
        out(sample.code, "\n");
    };
};
