type row_t: uint(bits: 4) [1 .. 3];
type rows_t: list of row_t;
struct sample_s {
    rows: rows_t;
    lower: list of int;
    upper: list of int;
    fixed: list of int;
    keep (rows.size() in [4 + 3 .. 10]) && (8 < rows.size());
    keep 5 <= lower.size();
    keep upper.size() < 3;
    keep 2 + 3 == fixed.size();
    regenerate(): uint is {
        gen rows;
        return rows.size();
    };
};
extend sys {
    run() is also {
        var sample: sample_s;
        gen sample;
        print sample.rows.size(), sample.lower.size(), sample.upper.size(), sample.fixed.size();
        print sample.regenerate(), sample.rows.count(it in [1 .. 3]);
    };
};
