struct packet_s { n: uint; };
struct owner_s {
    static n: uint;
    p: packet_s;
    accept(v: uint): uint is { return v; };
    static accept_static(v: uint): uint is { return v; };
    result(): uint is { return evaluate { value = "bad"; }; };
    check() is {
        var n: uint = evaluate { value = "bad"; };
        n = evaluate { value = "bad"; };
        p.n = evaluate { value = "bad"; };
        var xs: list of uint = { 0; };
        xs[0] = evaluate { value = "bad"; };
        owner_s::n = evaluate { value = "bad"; };
        accept(evaluate { value = "bad"; });
        owner_s::accept_static(evaluate { value = "bad"; });
    };
};
