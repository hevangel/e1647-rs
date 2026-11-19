struct child_s { n: uint; };
struct store_s {
    static count: uint;
    static !label: string = "x";
    static box: child_s;
    static xs: list of uint;
};
struct owner_s {
    check() is {
        var a: string = store_s::count;
        store_s::count = "bad";
        var b := store_s::box.absent;
        var c := store_s::count[0];
        var e := store_s::label * 2;
        var f: bool = store_s::xs[0];
    };
};
