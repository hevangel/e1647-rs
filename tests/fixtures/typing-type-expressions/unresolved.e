struct node_s { !key: uint; };
template struct value_box of (<n'exp>: uint) { !stored: uint; };
extend sys {
    !node: node_s;
    run() is also {
        var typed: typeof(node.missing);
        var item: typeof_item(node.other);
        var boxed: value_box of ((node.value_width));
    };
};
