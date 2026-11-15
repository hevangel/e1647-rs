struct packet_s {
    static !created: uint = 0;
    !n: uint = 0;
    init() is also { packet_s::created = packet_s::created + 1; };
};
type packet_alias: packet_s;
extend sys {
    event ready;
    !gate: semaphore;
    produce(p: packet_alias): packet_alias @sys.ready is {
        gate.down();
        sync;
        check that p.n == 6;
        return new with { n = 7; };
    };
    produce(p) is also {
        sync;
        check that p.n == 6;
        return new with { n = 8; };
    };
    bridge(p: packet_alias): packet_alias @sys.any is {
        var saved: uint = p.n;
        compute produce(p);
        check that saved == 6;
        return new with { n = 9; };
    };
    driver() @sys.any is {
        wait [2]*cycle;
        emit ready;
        sync;
        gate.up();
    };
    run() is also {
        gate = new semaphore;
        var baseline: uint = packet_s::created;
        start driver();
        compute me.bridge(new with { n = 6; });
        check that packet_s::created == baseline + 4;
        out("entry alias contexts passed");
    };
};
