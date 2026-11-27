type hue_t: [green, red];
type size_t: [small, large];
struct packet_s {
    !color: hue_t = red;
    !size: size_t = large;
    when red'color large'size {
        !payload: uint = 7;
    };
};
extend sys {
    !packet: packet_s;

    run() is also { out(packet.red'size'large'color'payload); };
    check() is also { packet.red'size'large'color'payload = 31; };
};
