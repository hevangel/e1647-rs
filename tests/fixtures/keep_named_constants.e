define LOWER 5;
define UPPER LOWER + 2;
define SQUARE 81;
define DIVISOR 3;
define shared 7;
struct sample_s {
    code: uint(bits: 4);
    shared: bit;
    rows: list of int;
    keep code * code == SQUARE;
    keep code / DIVISOR == DIVISOR;
    keep shared == UPPER;
    keep MAX_INT > SQUARE;
    keep rows.size() in [LOWER .. UPPER];
};
struct item_s {
    code: uint(bits: 40);
};
struct holder_s {
    payload: item_s;
    keep payload.code == UPPER;
    regenerate(): uint(bits: 40) is {
        gen payload;
        return payload.code;
    };
};
extend sys {
    run() is also {
        var sample: sample_s;
        var holder: holder_s = new;
        gen sample;
        gen holder.payload;
        print sample.code, sample.shared <= 1, sample.rows.size() in [LOWER .. UPPER];
        print holder.payload.code, holder.regenerate();
    };
};
