struct packet_s {
    static !created: uint = 0;
    !n: uint = 0;
    init() is also { packet_s::created = packet_s::created + 1; };
};
extend sys {
    event sampled;
    !gate: semaphore;
    !a: packet_s;
    !b: packet_s;
    !finished: uint = 0;
    inner(p: packet_s): packet_s is {
        var saved: uint = evaluate { value = p.n; };
        gate.down();
        wait rise(p.n) @sys.sampled;
        check that saved == 0;
        p = new with { n = 13; };
        check that p.n == 13;
        return new with { n = 14; };
    };
    inner(p) is also {
        sync;
        check that p.n > 0;
        return new with { n = 15; };
    };
    outer(p: packet_s, tag: uint): packet_s @sys.any is {
        var saved: uint = tag;
        inner(p);
        check that saved == tag;
        finished = finished + 1;
        return new with { n = 16; };
    };
    driver() @sys.any is {
        wait [2]*cycle;
        gate.up();
        gate.up();
        sync;
        emit sampled;
        sync;
        sync;
        a.n = 2;
        b.n = 4;
        emit sampled;
    };
    run() is also {
        gate = new semaphore;
        a.n = 0;
        b.n = 0;
        var baseline: uint = packet_s::created;
        start outer(a, evaluate { value = 1; });
        start outer(b, evaluate { value = 2; });
        start driver();
        wait [12]*cycle;
        check that finished == 2;
        check that packet_s::created == baseline + 8;
        check that a.n == 2;
        check that b.n == 4;
        out("nested edge contexts passed");
    };
};
