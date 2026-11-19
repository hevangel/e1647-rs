struct child_s { flag: string; };
struct owner_s {
    static flag: bool;
    static build() is {
        var a := new child_s with { var n: uint = flag; };
        var b: child_s = new with { var n: uint = flag; };
        var c := evaluate uint { value = flag; };
    };
};
