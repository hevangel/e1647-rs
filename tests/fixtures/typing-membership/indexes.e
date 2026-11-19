struct child_s { !n: uint; };
extend sys {
    xs: list of uint;
    child: child_s;
    run() is also {
        var a := xs["bad"];
        var b := xs[TRUE];
        var c := xs[1.5];
        var d := xs[NULL];
        var e := xs[child];
        var f := xs[xs];
        var g := xs[[1..3]];
        xs["write"] = 1;
        var h := (3)["bad"];
    };
};
