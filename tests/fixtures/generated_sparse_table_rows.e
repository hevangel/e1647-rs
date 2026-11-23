define LOW_CODE 34359738368;
define MID_CODE 274877906944;
define HIGH_CODE 549755813888;
type nearby_t: uint(bits: 40) [MID_CODE .. MID_CODE + 1];
struct sample_s {
    code: uint(bits: 40);
    partner: uint(bits: 40);
    !configured: uint(bits: 40) = HIGH_CODE;
    guarded: uint(bits: 40);
    keep (code, partner) in_table {
        [LOW_CODE, HIGH_CODE], LOW_CODE;
        set_of_values(nearby_t), HIGH_CODE;
    };
    keep guarded == HIGH_CODE;
    keep (guarded) in_table { LOW_CODE; configured; };
};
extend sys {
    run() is also {
        var sample: sample_s;
        gen sample;
        print sample.code, sample.partner, sample.guarded;
    };
};
