struct packet_s { n: uint; };
struct owner_s {
    check() is {
        var base_items := { 1; 2; };
        var joined := { base_items; 3; };
        var wrong: string = joined[0];
        var packets: list of packet_s = { new with { n = 1; }; { new with { n = 2; }; }; };
    };
};
