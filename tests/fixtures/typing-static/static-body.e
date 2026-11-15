struct owner_s {
    static flag: bool;
    label: string;
    static get(n: uint): uint is {
        var a: uint = flag;
        var b: uint = label;
        return "bad";
    };
};
