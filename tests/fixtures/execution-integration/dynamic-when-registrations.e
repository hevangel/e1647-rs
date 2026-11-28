type mode_t: [active, idle];
unit probe_u {
    !mode: mode_t = idle;
    !base_value: uint(bits: 2) = 1;
    event base_tick;
    cover base_tick is { item base_value; };
    when active'mode {
        !reading: uint(bits: 2) = 2;
        edge$: in event_port is instance;
        keep edge$.hdl_path() == "top.clock";
        keep edge$.edge() == rise;
        on edge$ { out("edge ", reading, "\n"); };
        cover edge$ using per_unit_instance is { item reading; };
        event request;
        event response;
        expect not @edge$ else { out("rule ", reading, "\n"); };
        expect @request => eventually @response else { out("stale rule\n"); };
    };
};
extend sys {
    probe: probe_u is instance;
    run() is also {
        set_config(cover, mode, normal);
        emit probe.base_tick;
        probe.mode = active;
        emit probe.request;
        wait [3]*cycle;
        probe.mode = idle;
        emit probe.base_tick;
        wait [2]*cycle;
        probe.mode = active;
        wait [3]*cycle;
    };
};
