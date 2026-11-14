struct child_s {
    value: uint;
    label: string;
};
struct owner_s {
    value: bool;
    make() is {
        var value: bool = TRUE;
        var a := new child_s with { value = "bad"; label = 3; };
        var b: child_s = new with { value = 0; var bad: uint = "bad"; };
        var restored: bool = value;
    };
};
