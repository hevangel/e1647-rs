extend sys {
    twice(n: uint): uint is {
        var copy := n;
        return copy + copy;
    };
    run() is also {
        var n := twice(4);
        var values := {1; 2};
        var element := values[0];
        var choice := TRUE ? "yes" : "no";
        out(n, " ", element, " ", choice, "\n");
    };
};
