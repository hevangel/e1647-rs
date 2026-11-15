struct owner_s {
    flag: bool;
    check() is {
        var a := evaluate uint (flag) { flag = "bad"; };
        var b := evaluate bool { value = 3; };
        var c: string = evaluate uint { value = 1; };
        var e: uint = evaluate (flag) { flag = 1; var bad: uint = "bad"; };
        var restored: bool = flag;
    };
};
