extend sys {
    run() is also {
        var values: list of int = {2; 3; 5};
        out(values.sum(), "\n");
    };
};
