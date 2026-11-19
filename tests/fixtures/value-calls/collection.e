extend sys {
    !calls: uint = 0;
    later(n: uint): uint is { calls = calls + 1; sync; return n; };
    items(): list of uint is { sync; return { 2; 4; }; };
    run() is also {
        var values: list of uint = { later(1); later(2); };
        values[later(0)] = later(7);
        print later(8), later(9);
        for each (item) in items() { out(item, "\n"); sync; };
        out(values[0], " ", values[1], " calls ", calls, "\n");
    };
};
