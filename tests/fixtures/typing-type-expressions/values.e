struct node_s { !key: uint; };
extend sys {
    !node: node_s;
    run() is also {
        var local: uint(bits: node.missing);
        var result: int = evaluate uint(bits: node.other) { value = 0; };
        var values: set = set_of_values(uint(bits: node.absent));
        var full: set = full_set_of_values(uint(bits: "wide"));
        var matched: bool = (0) in_table { set_of_values(uint(bits: node.nope)); };
        var casted := (0).as_a(uint(bits: node.cast_width));
        var tested: bool = 0 is a uint(bits: node.test_width);
        var made := new uint(bits: node.new_width);
    };
};
