type width_t: uint(bits: 8);
struct node_s { !key: uint; };
template struct box_t of <type> { !item: <type>; };
struct owner_s {
    !it: string;
    !later: string;
    !keys: list (key: it.key) of node_s;
    takes(p: uint(bits: later), later: int): uint(bits: later) is {
        var it: string = "outer";
        var nested: list (key: it.key) of node_s;
        var after: string = it;
        var unknown: uint(bits: mystery);
        var boxed: box_t of (uint(bits: later));
        return 0;
    };
};
