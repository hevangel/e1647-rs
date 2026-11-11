extend sys {
    q: event;
    !level: int = 10;
    expect eventually (rise(level) @q) else dut_error("missed sampled rise");
    expect not (fall(level) @q) else dut_error("sampled an intermediate value");
    run() is also {
        emit q;
        wait [1]*cycle;
        level = 100;
        wait [1]*cycle;
        level = 20; emit q;
        wait [1]*cycle;
        out("sampling complete\n");
    };
};
