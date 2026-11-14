struct child_s {
    n: uint;
    accept(v: uint): uint is { return v; };
};
struct owner_s {
    label: string;
    obj: child_s;
    accept(v: uint): uint is { return v; };
    check() is {
        accept(accept(label * 2));
        obj.accept(obj.accept(label * 2));
        (obj.absent)(label * 2);
    };
};
