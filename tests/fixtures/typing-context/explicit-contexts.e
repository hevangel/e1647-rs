struct packet_s { n: uint; };
struct other_s { flag: bool; };
struct owner_s {
    check() is {
        var n: uint = evaluate bool { value = FALSE; };
        var p: packet_s = new other_s with { flag = 1; };
        var xs: list of packet_s = new packet_s;
    };
};
