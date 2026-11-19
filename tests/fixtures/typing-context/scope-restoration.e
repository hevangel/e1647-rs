struct packet_s { n: uint; };
struct owner_s {
    value: bool;
    check() is {
        var value: string = "outer";
        var n: uint = evaluate (value) { value = "bad"; };
        var restored: string = value;
        var p: packet_s = new with { n = "bad"; };
        var still_restored: string = value;
        var flag: bool = TRUE;
    };
};
