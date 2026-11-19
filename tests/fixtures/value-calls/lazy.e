extend sys {
    !calls: uint = 0;
    number(n: uint): uint is { calls = calls + 1; sync; return n; };
    flag(): bool is { calls = calls + 1; sync; return TRUE; };
    run() is also {
        var a: bool = FALSE && flag();
        var b: bool = TRUE || flag();
        var n: uint = TRUE ? number(7) : number(99);
        if flag() { out(a, " ", b, " ", n, " calls ", calls, "\n"); };
        case number(2) { number(1): { out("wrong\n"); }; number(2): { sync; out("case ", calls, "\n"); }; };
    };
};
