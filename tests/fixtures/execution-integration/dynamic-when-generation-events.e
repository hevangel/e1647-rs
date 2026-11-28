type mode_t: [alpha, beta, idle];
struct leaf_s {
    value: uint(bits: 4);
    keep value == 6;
};
struct packet_s {
    !mode: mode_t = alpha;
    event common;
    on common { out("base\n"); };
    when alpha'mode {
        value: uint(bits: 4);
        leaves: list of leaf_s;
        tagged: bool;
        keep value == 3;
        keep leaves.size() == 2;
        keep tagged == TRUE;
        event done;
        on done { out("alpha ", value, "\n"); };
        on common { out("alpha common\n"); };
        observe() @done is { out("alpha waiter\n"); };
        when tagged {
            nested: uint(bits: 4);
            keep nested == 7;
            on common { out("nested ", nested, "\n"); };
        };
    };
    when beta'mode {
        value: uint(bits: 4);
        keep value == 9;
        event done;
        on done { out("beta ", value, "\n"); };
        on common { out("beta common\n"); };
    };
};
extend sys {
    packet: packet_s;
    run() is also {
        emit packet.common;
        out(packet.alpha'value, " ", packet.alpha'leaves.size(), " ", packet.alpha'leaves[0].value, "\n");
        start packet.observe();
        packet.mode = beta;
        packet.beta'value = 12;
        gen packet;
        emit packet.done;
        emit packet.common;
        wait cycle;
        packet.mode = idle;
        emit packet.common;
        packet.mode = alpha;
        out(packet.alpha'value, "\n");
        emit packet.done;
        wait cycle;
    };
};
