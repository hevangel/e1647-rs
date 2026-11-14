struct child_s { n: uint; };
struct owner_s {
    label: string;
    n: uint;
    xs: list of uint;
    obj: child_s;
    check() is {
        var a := (label * 2).as_a(uint);
        var b := xs[obj.absent];
        var c := (label * 2) is a uint;
        var e := n[(label * 2):0];
    };
};
