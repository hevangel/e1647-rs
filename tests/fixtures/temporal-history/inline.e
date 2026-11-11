extend sys {
    !flag: bool = TRUE;
    expect {true(flag); true(not flag)} else dut_error("lost inline history");
    inner() @sys.any is {
        wait [1]*cycle;
        flag = FALSE;
        wait [1]*cycle;
    };
    run() is also {
        compute inner();
        out("inline complete\n");
    };
};
