struct packet_s {
    n: uint;
    init() is also { (new); };
};
extend sys {
    run() is also { var p: packet_s = evaluate { value.n = 1; }; };
};
