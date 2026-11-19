type state_t: [active_state, idle_state];
struct packet_s {
    !flag: state_t = active_state;
    !hFF: int = 7;
    !bX: int = 9;
    face(): int is { return hFF; };
    when active_state'flag { !extra: int = 3; };
};
extend sys {
    run() is also {
        var p: packet_s = new packet_s;
        check that p.active_state'hFF == 7;
        check that p.active_state'bX == 9;
        check that p.active_state'face() == 7;
        check that 'hFF == 255;
        check that 'b1010 == 10;
        check that 'o17 == 15;
        check that 'd15 == 15;
        check that 8'hFF == 255;
        check that 0xFF == 255;
        out("tick qualifiers passed");
    };
};
