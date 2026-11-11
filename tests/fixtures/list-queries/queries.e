extend sys {
    run() is also {
        var xs: list of int = {3; 1; 3; 2};
        var it: string = "outer";
        out(xs.has(it == 3), " ", xs.has(it > 9), "\n");
        out(xs.count(it == 3), " ", xs.count(it > 9), "\n");
        out(xs.all(it > 1), "\n");
        out(xs.all_indices(it == 3), "\n");
        out(xs.count({2; 3}.has(it == 3) and it == 3), "\n");
        out(it, " ", xs.size(), "\n");
        var none: list of int;
        out(none.has(it > 0), " ", none.count(it > 0), "\n");
        out(none.all(it > 0).size(), " ", none.all_indices(it > 0).size(), "\n");
        var selected: list of int = xs.all(TRUE);
        out(selected list_ptr== xs, "\n");
        selected.push(8);
        out(xs.size(), " ", selected.size(), "\n");
    };
};
