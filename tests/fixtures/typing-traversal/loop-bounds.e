struct child_s { n: uint; };
struct owner_s {
    label: string;
    obj: child_s;
    xs: list of uint;
    check() is {
        for i from label * 2 to obj.absent { };
        for each (item) in xs[obj.absent] { print label * 2; };
    };
};
