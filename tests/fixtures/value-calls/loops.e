extend sys {
    !n: uint = 0;
    !checks: uint = 0;
    more(): bool is { checks = checks + 1; sync; return n < 3; };
    done(): bool is { checks = checks + 1; sync; return n == 5; };
    bound(n: uint): uint is { sync; return n; };
    run() is also {
        while more() { n = n + 1; sync; };
        out("while ", n, " checks ", checks, "\n");
        repeat { n = n + 1; sync; } until done();
        out("repeat ", n, " checks ", checks, "\n");
        for i from bound(1) to bound(2) { out(i, "\n"); sync; };
    };
};
