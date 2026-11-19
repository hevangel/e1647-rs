struct packet_s { n: uint; };
extend sys {
    run() is also { var p: packet_s = new with { var n := evaluate { value = 1; }; }; };
};
