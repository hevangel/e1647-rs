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
        xs.and_all(it);
        xs.or_all(it);
        xs.first(it);
        var bad_index: string = xs.last_index(it > 0);
        objects.sort_by_field(absent);
        xs.get_indices(objects);
        xs.resize("bad");
        xs.resize(3, "bad");
        xs.resize(3, UNDEF, 1);
    };
};
