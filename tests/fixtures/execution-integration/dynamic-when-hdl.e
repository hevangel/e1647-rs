type mode_t: [active, idle];
struct monitor_s {
    !mode: mode_t = active;
    when active'mode {
        event tick is rise('top.clock') @sim;
        on tick { out("rise\n"); };
    };
};
extend sys {
    !monitor: monitor_s;
    run() is also {
        wait [2]*cycle;
        monitor.mode = idle;
        wait [2]*cycle;
        out("reactivate\n");
        monitor.mode = active;
        wait [2]*cycle;
    };
};
