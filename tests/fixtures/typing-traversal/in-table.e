struct child_s { n: uint; };
struct owner_s {
    label: string;
    obj: child_s;
    check() is {
        var a := (obj.absent, 0) in_table { 0, [0..(label * 2)]; 1, (label * 2); };
    };
};
