struct child_s { n: uint; };
struct owner_s {
    label: string;
    n: uint;
    obj: child_s;
    keep n == select { (label * 2): 1; };
    check() is {
        var a := [obj.absent, 1];
        var b := [(label * 2)..5];
    };
};
