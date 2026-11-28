extend sys {
    snapshot(label: string) is {
        out(label, ": count ", 'event_ordering.count',
            ", edge tick ", 'event_ordering.edge_tick',
            ", delayed ", 'event_ordering.delayed_count', "\n");
    };
    run() is also {
        hdl_load("PROFILE_DESIGN", "event_ordering", "xezim");
        hdl_run(1);
        snapshot("initial");

        'event_ordering.clk' = 1;
        snapshot("first write");
        hdl_run(0);
        snapshot("first zero step");
        hdl_run(0);
        snapshot("repeated zero step");
        hdl_run(1);
        snapshot("first one tick");

        'event_ordering.clk' = 0;
        hdl_run(0);
        'event_ordering.clk' = 1;
        snapshot("second write");
        hdl_run(1);
        snapshot("second one tick");
        hdl_run(0);
        snapshot("second target zero step");
        hdl_finish();
    };
};
