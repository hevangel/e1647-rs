type bad_width_t: uint(bits: TRUE);
struct node_s { !key: uint; };
struct owner_s {
    !node: node_s;
    !width_bad: uint(bits: "wide");
    !keys: list (key: it.missing) of node_s;
    !nested: list of uint(bits: node.missing);
    takes(p: uint(bits: later.missing), later: node_s): uint(bits: later.other) is {
        var local: uint(bits: later.absent);
        return 0;
    };
    static !config: node_s;
    static takes_static(p: uint(bits: config.missing)): int is { return 0; };
};
extend sys {
    cover cg is { item value: uint(bits: "bad") = 0; };
};
