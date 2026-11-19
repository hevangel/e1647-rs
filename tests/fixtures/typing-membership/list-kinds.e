extend sys {
    ns: list of uint;
    ss: list of string;
    run() is also {
        var a := "bad" in ns;
        var b := ns in ss;
        var c := ss in { 1; 2; };
        var d := 1 in { "bad"; "other"; };
        var e := TRUE in ns;
        var f := ns not in ss;
    };
};
