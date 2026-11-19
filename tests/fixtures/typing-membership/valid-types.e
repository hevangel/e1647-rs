type color_t: [red_t, green_t, blue_t];
type numbers_t: list of uint;
extend sys {
    ns: numbers_t;
    ss: list of string;
    es: list of color_t;
    check(index: color_t) is {
        var a := ns[index];
        var b := ns in { 1; 2; };
        var c := 1 in ns;
        var d := ss in { "a"; "b"; };
        var e := red_t in es;
        var f := es in [red_t..blue_t];
        var g := TRUE in set_of_values(bool);
        var h := 1.5 in { 1.5; 2.5; };
        var i := { 1; 1; } in [1..3];
        var j: set = full_set_of_values(color_t);
        var k := j in { j; };
    };
};
