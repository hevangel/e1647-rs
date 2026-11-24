struct seeded_leaf_s {
    code: uint(bits: 8);
    keep code in [12 .. 19];
};
struct seeded_nested_s {
    leaf: seeded_leaf_s;
    digits: list of uint(bits: 6);
    enabled: bool;
    keep digits.size() == 3;
};
struct seeded_sample_s {
    serial_number: uint(bits: 40);
    nested: seeded_nested_s;
    batch: list of seeded_leaf_s;
    keep batch.size() == 2;
};
extend sys {
    sample: seeded_sample_s;
    run() is also {
        out(sample, "\n");
        gen sample;
        out(sample, "\n");
        gen sample;
        out(sample, "\n");
    };
};
