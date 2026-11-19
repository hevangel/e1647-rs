struct child_s { n: uint; };
struct owner_s {
    label: string;
    n: uint;
    xs: list of uint;
    obj: child_s;
    check() is {
        var a := (label * 2) in [0, 1];
        var b := n in [obj.absent, 1];
        var c := n in [(label * 2)..3];
        var e := n in [0..(label * 2)];
        var f := n in xs[obj.absent];
    };
};
