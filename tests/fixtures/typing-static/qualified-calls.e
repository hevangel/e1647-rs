struct child_s { n: uint; };
struct api_s {
    static accept(n: uint): uint is { return n; };
    static object(): child_s is { return new child_s; };
};
struct owner_s {
    label: string;
    check() is {
        var a: string = api_s::accept(1);
        api_s::accept("bad");
        api_s::accept(label * 2);
        var b := api_s::object().absent;
        api_s::accept(api_s::accept(label * 2));
    };
};
