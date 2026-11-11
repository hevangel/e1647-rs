extend sys {
    !flag: bool = TRUE;
    !values: list of uint;
    !n: uint = 2;
    expect {true(flag); true(not flag)} else dut_error("lost flag history");
    expect {true(values[0] == 1); true(values[0] == 2)} else dut_error("aliased list history");
    expect [n]*cycle else dut_error("lost bound history");
    run() is also {
        values = {1};
        wait [1]*cycle;
        flag = FALSE;
        values[0] = 2;
        n = 9;
        wait [1]*cycle;
        out("rules complete\n");
    };
};
