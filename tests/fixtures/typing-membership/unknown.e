template struct box_t of <type> { value: <type>; };
extend sys {
    check(x, container) is {
        var a := x in container;
        var b := x["unknown receiver"];
        var c := x in { 1; 2; };
        var d := 1 in container;
        var e := { 1; } in container;
    };
};
