type mode_t: [alpha, beta];
struct packet_s {
    !mode: mode_t = alpha;
    shared: uint;
    keep shared >= 10;
    when alpha'mode {
        count: uint;
        ~window: uint(bits: 5);
        keep shared == 19;
        keep count == shared;
        keep window >= count;
        keep window <= count + 2;
    };
    when beta'mode {
        count: uint;
        ~window: uint(bits: 5);
        keep shared == 11;
        keep count == shared;
        keep window in [2..3];
    };
};
extend sys {
    packet: packet_s;
    run() is also {
        out(packet.shared, " ", packet.alpha'count, " ", packet.alpha'window, " ", rgf_get_range(packet.alpha'window), "\n");
        packet.alpha'window rgf= 21;
        packet.mode = beta;
        out(rgf_was_generated(packet.beta'window), "\n");
        gen packet;
        out(packet.shared, " ", packet.beta'count, " ", packet.beta'window, " ", rgf_get_range(packet.beta'window), "\n");
        packet.mode = alpha;
        out(packet.alpha'window, " ", rgf_was_reduced(packet.alpha'window), "\n");
    };
};
