define flag 1;
struct owner_s {
    static flag: bool;
    static !count: uint = flag;
    static read(flag: string): uint is {
        var a: uint = flag;
        var b: uint = unknown_static();
        var c: uint = owner_s::count;
        return unknown_value;
    };
};
