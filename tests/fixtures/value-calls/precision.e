extend sys {
    delayed(n: uint): uint is { sync; return n; };
    run() is also {
        var fixed: uint = 4294967295;
        var wide: longuint = 4294967295;
        var a: uint = fixed + delayed(1);
        var b: longuint = wide + delayed(1);
        out(a, " ", b, "\n");
    };
};
