struct child_s {};
extend sys {
    child: child_s;
    run() is also {
        var a := 1 in 2;
        var b := 1 in "bad";
        var c := 1 in FALSE;
        var d := 1 in NULL;
        var e := 1 in child;
    };
};
