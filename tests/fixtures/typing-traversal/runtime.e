struct child_s {
    value: uint;
    event tick;
};
extend sys {
    !value: bool = TRUE;
    run() is also {
        var value: bool = TRUE;
        var child: child_s = new child_s with { value = 7; };
        var evaluated := evaluate uint (value) { value = child.value; };
        var xs: list of uint = { evaluated; 8; };
        check that xs[evaluated - 7] == 7;
        check that evaluated in [6..8];
        check that (evaluated, 8) in_table { 7, [8..9]; };
        check that evaluated[3:0] == 7;
        check that value;
        emit child.tick;
        var i: instance_iterator = new instance_iterator;
        out("traversal scopes passed");
    };
};
