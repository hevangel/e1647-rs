define LOWER 34359738368;
define UPPER 549755813888;
struct sample_s {
    code: uint(bits: 40);
    interior: uint(bits: 40);
    nearest: uint(bits: 40);
    keep code in [1, LOWER, LOWER, UPPER];
    keep code >= LOWER;
    keep code in [2, LOWER, UPPER];
    keep interior in [5, LOWER, UPPER];
    keep soft interior == select { 1 : others; };
    keep nearest in [0, 100, UPPER];
    keep soft nearest == select { 1 : normal(75, 0); };
};
extend sys {
    run() is also {
        var sample: sample_s;
        gen sample;
        print sample.code, sample.interior, sample.nearest;
    };
};
