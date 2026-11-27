type color_t: [red, green];
type detail_t: [small, large];
struct packet_s {
    !color: color_t = red;
    !detail_mode: detail_t = small;
    when red'color {
        !label: uint = 17;
        count: uint(bits: 4);
        keep count == 7;
        when large'detail_mode {
            !detail: uint = 23;
        };
    };
};
extend sys {
    run() is also {
        var packet: packet_s = new packet_s;
        out(packet.red'color'label, " ", packet.red'color'count, "\n");
        packet.red'color'label = 19;
        packet.detail_mode = large;
        out(packet.red'color'label, " ", packet.red'large'detail, "\n");
        packet.detail_mode = small;
        out(packet.red'color'label, "\n");
    };
};
