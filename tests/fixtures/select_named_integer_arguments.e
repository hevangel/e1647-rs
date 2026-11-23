define CENTER 6 * 2;
define SPREAD 0;
define OFFSET MIN_INT + MAX_INT;
struct sample_s {
    first_value: int(bits: 8);
    second_value: int(bits: 8);
    overflow_value: int(bits: 8);
    keep soft first_value == select { 1 : normal(-(CENTER + 2), +SPREAD); };
    keep soft second_value == select { 1 : normal(OFFSET, 0); };
    keep overflow_value == 7;
    keep soft overflow_value == select { 1 : normal(170141183460469231731687303715884105727 + 1, 0); };
};
extend sys {
    run() is also {
        var sample: sample_s;
        gen sample;
        print sample.first_value, sample.second_value, sample.overflow_value;
    };
};
