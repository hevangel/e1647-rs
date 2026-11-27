type mode_t: [idle, active, extra];
type depth_t: [shallow, deep];
struct packet_s {
    !mode: mode_t = idle;
    !depth: depth_t = shallow;
    !count: uint = 0;

    sample(): uint is { return 1; };
    sample(): uint is also { out("after "); };
    record() is { count = count + 1; };
    delayed(): uint is { return 4; };
    pause_tick() @sys.any is { wait cycle; };

    when active'mode {
        sample(): uint is only { return 2; };
        record() is only { count = count + 5; };
        delayed(): uint is only {
            compute pause_tick();
            return 8;
        };
        when deep'depth {
            sample(): uint is only { return 3; };
        };
    };
    when extra'mode {
        !extra_value: uint = 20;
        sample(): uint is only { return extra_value; };
    };
};
struct child_s like packet_s {};
extend sys {
    !packet: packet_s;
    flip() @sys.any is {
        packet.mode = idle;
    };
    run() is also {
        var child: child_s = new child_s;
        out(packet.sample(), "\n");
        packet.mode = active;
        packet.record();
        out(packet.sample(), " ", packet.count, "\n");
        packet.depth = deep;
        out(packet.sample(), "\n");
        child.mode = active;
        out(child.sample(), "\n");
        packet.mode = extra;
        out(packet.sample(), "\n");
        packet.mode = active;
        start flip();
        out(packet.delayed(), " ", packet.delayed(), "\n");
    };
};
