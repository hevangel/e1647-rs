struct box_s {
    size: uint;
};
struct owner_s {
    take(v: uint) is empty;
    run() is {
        var n := 3;
        n = "bad";
        var alias := n;
        take("bad");
        var label := "hello";
        var wrong: uint = label;
        var object := new box_s;
        object.absent;
        alias[0];
    };
};
