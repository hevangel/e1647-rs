struct node_s { !key: int; };
extend sys {
    run() is also {
        var node: node_s;
        var number: int = 2;
        var sized: uint(bits: evaluate uint {
            var matched: bool = node is a node_s (number);
            value = 8;
        });
        var bad: string = number;
    };
};
