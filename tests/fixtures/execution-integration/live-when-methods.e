type mode_t: [idle, active, extra];
type depth_t: [shallow, deep];
struct packet_s {
    !mode: mode_t = idle;
    !depth: depth_t = shallow;
    !count: uint = 0;
    event base_go;
    event active_go;

    sample(): uint is { return 1; };
    sample(): uint is also { out("after "); };
    record() is { count = count + 1; };
    delayed(): uint is { return 4; };
    pause_tick() @sys.any is { wait cycle; };
    observe() @base_go is { out("base entry\n"); };

    when active'mode {
        !stored: uint = 7;
        sample(): uint is only { return 2; };
        record() is only { count = count + 5; };
        observe() @active_go is only {
            out("active entry ", stored, "\n");
            wait cycle;
            me.stored = stored + 1;
            out("active exit ", self.stored, "\n");
        };
        delayed(): uint is only {
            compute pause_tick();
            return 8;
        };
        when deep'depth {
            sample(): uint is only { return 3; };
        };
    };
    when extra'mode {
        !stored: uint = 20;
        sample(): uint is only {
            var value: uint = stored;
            stored = stored + 1;
            return value;
        };
    };
    when deep'depth {
        keep count == 100;
        sample(): uint is only { return count; };
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
        packet.mode = extra;
        out(packet.sample(), " ", packet.extra'stored, "\n");
        packet.mode = active;
        start packet.observe();
        packet.mode = idle;
        emit packet.base_go;
        wait cycle;
        out("waiting\n");
        emit packet.active_go;
        wait [3]*cycle;
        packet.mode = active;
        emit packet.active_go;
        compute packet.observe();
        out(packet.active'stored, "\n");
        var copied: packet_s = deep_copy(packet);
        copied.active'stored = 10;
        out(packet.active'stored, " ", copied.active'stored, "\n");
        copied.mode = extra;
        out(copied.extra'stored, "\n");
    };
};
