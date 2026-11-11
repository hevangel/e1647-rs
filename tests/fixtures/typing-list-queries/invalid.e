struct item_s {
    value: uint;
};
extend sys {
    run() is also {
        var xs: list of int;
        var objects: list of item_s;
        xs.has(it);
        xs.count(it + "bad" == 0);
        objects.all(it.absent == 0);
        var wrong: string = xs.count(it > 0);
        var outer: int = 1;
        var it: string = "outer";
        xs.has(it > outer);
        var still_string: string = it;
    };
};
