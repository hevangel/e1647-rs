extend sys {
    run() is also {
        var xs: list of int = {1; 2};
        out(xs.max(xs[8]));
    };
};
