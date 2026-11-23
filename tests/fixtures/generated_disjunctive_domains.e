define LOW_EDGE 34359738368;
define HIGH_EDGE 549755813888;
struct sample_s {
    first_value: uint(bits: 40);
    second_value: uint(bits: 40);
    guarded_value: uint(bits: 40);
    enabled(): bool is { return TRUE; };
    keep ((first_value == LOW_EDGE && second_value == HIGH_EDGE)
          or (first_value == HIGH_EDGE && second_value == LOW_EDGE));
    keep guarded_value == HIGH_EDGE;
    keep (guarded_value == LOW_EDGE or enabled());
};
extend sys {
    run() is also {
        var sample: sample_s;
        gen sample;
        print sample.first_value, sample.second_value, sample.guarded_value;
    };
};
