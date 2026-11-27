type hue_t: [green, red];
type size_t: [small, large];
struct packet_s {
    !hue: hue_t = green;
    !size_mode: size_t = small;
    when red'hue {
        !payload: uint = 17;
        when large'size_mode { !detail: uint = 23; };
    };
    when MARKER packet_s { !tag: uint = 3; };
};
struct child_s like packet_s {};
struct unrelated_s {
    !hue: hue_t = red;
    !size_mode: size_t = large;
};
extend sys {
    run() is also {
        var packet: packet_s = new packet_s;
        var child: child_s = new child_s;
        var unrelated: unrelated_s = new unrelated_s;
        var declared: red packet_s = new red packet_s;
        var marked: MARKER packet_s = new MARKER packet_s;

        out(packet is a red packet_s, " ", declared is a red packet_s, "\n");
        out(packet is a MARKER packet_s, " ", marked is a MARKER packet_s, "\n");
        packet.hue = red;
        if packet is a red packet_s (matched) {
            out(matched.red'payload, "\n");
        };
        out(packet is a red large packet_s, "\n");
        packet.size_mode = large;
        var size_matches: bool = packet is a large packet_s;
        out(packet is a red large packet_s, " ", size_matches, "\n");

        child.hue = red;
        out(child is a red packet_s, " ", unrelated is a red packet_s, "\n");
        packet.hue = green;
        declared.hue = green;
        out(packet is a red packet_s, " ", declared is a red packet_s, "\n");
    };
};
