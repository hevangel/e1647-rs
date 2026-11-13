type mode_t: [cold, warm, hot];
extend sys {
    !mode: mode_t = cold;
    expect eventually rise(mode) else dut_error("no ordinal increase");
    expect eventually fall(mode) else dut_error("no ordinal decrease");
    expect eventually change(mode) else dut_error("no ordinal change");
    run() is also {
        wait [1]*cycle;
        mode = hot;
        wait [1]*cycle;
        mode = warm;
        wait [1]*cycle;
        out("enumeral complete\n");
    };
};
