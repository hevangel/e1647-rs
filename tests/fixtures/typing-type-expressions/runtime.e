type byte_t: uint(bits: 8);
struct node_s { !key: byte_t = 3; };
extend sys {
    identity(p: uint(bits: 8)): uint(bits: 8) is { return p; };
    run() is also {
        var value: uint(bits: 8) = identity(7);
        var result: int = evaluate uint(bits: 8) { value = 2; };
        var nodes: list (key: it.key) of node_s = {new node_s};
        var small: set = set_of_values(uint(bits: 2));
        check that value == 7;
        check that result == 2;
        check that nodes[0].key == 3;
        check that 3 in small;
        check that not (4 in small);
        out("type expressions passed");
    };
};
