type small_t: int(bits: 8) [-9 .. -2];
type alias_t: small_t;
struct sample_s {
    first_value: alias_t;
    second_value: alias_t;
    values: list of alias_t;
    ~reduced_value: alias_t;
    keep first_value >= -5;
    keep soft first_value == select { 100 : min; };
    keep second_value == -4;
    keep values.size() == 2;
    keep reduced_value > -7;
};
extend sys {
    run() is also {
        var sample: sample_s;
        gen sample;
        print sample.first_value, sample.second_value, sample.reduced_value;
        print sample.values.size(), sample.values.count(it in [-9 .. -2]);
    };
};
