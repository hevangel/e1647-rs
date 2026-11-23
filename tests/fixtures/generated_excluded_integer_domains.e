define LAST 1099511627775;
struct sample_s {
    edge: uint(bits: 40);
    final_value: uint(bits: 40);
    signed_edge: int(bits: 40);
    table_value: uint(bits: 40);
    keep edge not in [1 .. LAST - 1];
    keep final_value not in [1 .. LAST - 1] && final_value not in [0, 0];
    keep not (signed_edge in [-549755813887 .. 549755813886]);
    keep soft signed_edge == select { 1 : min; };
    keep table_value in [0, LAST];
    keep (table_value) not in_table { 0; };
};
extend sys {
    run() is also {
        var sample: sample_s;
        gen sample;
        print sample.edge, sample.final_value, sample.signed_edge, sample.table_value;
    };
};
