extend sys {
    !level: int = -4;
    !flag: bool = FALSE;
    !unchanged: uint = 7;
    expect eventually rise(level) else dut_error("no numeric increase");
    expect eventually fall(level) else dut_error("no numeric decrease");
    expect eventually change(flag) else dut_error("no boolean change");
    expect eventually rise(flag) else dut_error("no boolean rise");
    expect eventually (fall(flag) and true(level == -9)) else dut_error("no boolean fall");
    expect not change(unchanged) else dut_error("baseline incorrectly fired");
    run() is also {
        wait [1]*cycle;
        level = -3; flag = TRUE;
        wait [1]*cycle;
        level = -9; flag = FALSE;
        wait [1]*cycle;
        out("numeric complete\n");
    };
};
