extend sys {
    run() is also {
        var xs: list of int = {4; 1; 4; 2};
        var it: string = "outer";
        out(xs.max(-it), " ", xs.min(-it), "\n");
        out(xs.max_index(it), " ", xs.min_index(-it), "\n");
        out(xs.max(it + {2; 5}.min(it)), " ", it, "\n");
        out({1.5; 3.5; 2.0}.min(-it), "\n");
        out(xs.max_index("same"), " ", xs.min_index("same"), "\n");
        out(xs.max(), " ", xs.min(), "\n");
        out(xs.size(), " ", it, "\n");
    };
};
