struct child_s {};
type set_alias: set;
extend sys {
    s: set;
    alias: set_alias;
    child: child_s;
    run() is also {
        var a := "bad" in s;
        var b := child in alias;
        var c := NULL in [1..3];
        var d := child in ["bad"..3];
        var e := "bad" in set_of_values(bool);
        var f := child in [1,2];
        var g: uint = [1..3];
    };
};
