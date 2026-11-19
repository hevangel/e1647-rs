struct packet_s { n: uint; };
struct owner_s {
    p: packet_s;
    accept(v: packet_s) is { };
    result(): packet_s is { return new with { n = "bad"; }; };
    check() is {
        var p: packet_s = (new with { n = "bad"; });
        p = new with { n = "bad"; };
        accept(new with { n = "bad"; });
        var xs: list of packet_s = { new with { n = "bad"; }; };
        xs[0] = new with { n = "bad"; };
        var q: packet_s = TRUE ? new with { n = "bad"; } : new packet_s;
    };
};
