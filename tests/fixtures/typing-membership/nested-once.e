struct child_s {};
extend sys {
    child: child_s;
    xs: list of uint;
    run() is also {
        var a := xs[child.missing];
        var b := xs["bad" + 1];
        var c := child.missing in xs;
        var d := 1 in { child.missing; };
        var e := child.missing in [child.missing..3];
    };
};
