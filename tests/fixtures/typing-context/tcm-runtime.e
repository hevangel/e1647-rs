struct packet_s {
    static !created: uint = 0;
    !n: uint = 0;
    init() is also { packet_s::created = packet_s::created + 1; };
};
unit worker_u {
    !seen: uint = 0;
    consume(p: packet_s) is { wait [1]; seen = p.n; };
    produce(): packet_s is { wait [1]; return new with { n = 7; }; };
};
extend sys {
    w: worker_u is instance;
    run() is also {
        start w.consume(new with { n = 6; });
        start w.produce();
        wait [3];
        check that w.seen == 6;
        check that packet_s::created == 2;
        out("suspended contexts passed");
    };
};
