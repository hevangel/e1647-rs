type mode_t: [alpha, beta];
struct packet_s {
    !mode: mode_t = alpha;
    !allow: bool = TRUE;
    when alpha'mode {
        value: uint(bits: 4);
        keep value == 5;
        keep allow;
        event alpha_only;
        on alpha_only { out("alpha event\n"); };
    };
    when beta'mode {
        value: uint(bits: 4);
        keep value == 8;
    };
};
extend sys {
    packet: packet_s;
    run() is also {
        packet.alpha'value = 13;
        packet.allow = FALSE;
        gen packet;
    };
    check() is also {
        out(packet.alpha'value, "\n");
        packet.mode = beta;
        emit packet.alpha_only;
    };
};
