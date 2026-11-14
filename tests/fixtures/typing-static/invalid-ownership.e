struct base_s {
    static n: uint;
    read(): uint is { return 1; };
};
struct child_s like base_s { };
struct owner_s {
    check() is {
        var a: string = child_s::n;
        var b: string = base_s::read();
    };
};
