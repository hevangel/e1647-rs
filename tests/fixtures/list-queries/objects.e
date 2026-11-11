struct item_s {
    value: uint;
};
extend sys {
    !calls: uint = 0;
    xs: list of int;
    predicate(v: int): bool is {
        calls += 1;
        xs.clear();
        return v > 1;
    };
    run() is also {
        var a: item_s = new item_s;
        a.value = 7;
        var objects: list of item_s;
        objects.push(a);
        var selected: list of item_s = objects.all(it.value == 7);
        selected[0].value = 9;
        out(a.value, "\n");
        xs = {1; 2; 3};
        out(xs.count(predicate(it)), " ", calls, " ", xs.size(), "\n");
        xs = {2; 3}; calls = 0;
        out(xs.has(predicate(it)), " ", calls, "\n");
    };
};
