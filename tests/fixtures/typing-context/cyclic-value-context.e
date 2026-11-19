extend sys { !xs: uint;
    run() is also {
        var xs := { 0; };
        xs[0] = xs;
        xs = evaluate { value = { 1; }; };
    };
};
