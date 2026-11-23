struct sample_s {
    preferred: uint(bits: 4);
    other: uint(bits: 4);
    keep (preferred == select { 100 : min; }) && (other * other == 9);
    regenerate(): uint is {
        preferred = 8;
        gen preferred;
        return preferred;
    };
};
extend sys {
    run() is also {
        var sample: sample_s;
        gen sample;
        print sample.preferred, sample.other, sample.regenerate();
    };
};
