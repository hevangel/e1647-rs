type state_t: [active_state, idle_state];
struct packet_s {
    !flag: state_t = active_state;
    !data: int;
    !hFF: int;
    when active_state'flag { !extra: int; };
};
extend sys {
    run() is also {
        var p: packet_s;
        var bad: string = p.active_state'data;
        var worse: string = p.active_state'hFF;
    };
};
