struct required_s {
    code: uint(bits: 4);
    keep code == 3;
    keep soft code == 4;
    regenerate(): uint is {
        gen code;
        return code;
    };
};
struct preferred_s {
    code: uint(bits: 4);
    keep code in [1 .. 8];
    keep soft code == 7;
};
struct parity_s {
    code: uint(bits: 4);
    keep code >= 14;
    keep code % 2 == 0;
    keep soft code == select { 100 : max; };
};
extend sys {
    !entry: required_s;
    keep soft entry.code == 5;
    run() is also {
        var preferred: preferred_s;
        var parity: parity_s;
        gen entry;
        gen preferred;
        gen parity;
        print entry.code, entry.regenerate(), preferred.code, parity.code;
    };
};
