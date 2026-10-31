extend sys {
    event enter_exec_st is @sys.any;
    event fetch1_assert is (change(smp.sig_p$) and
                         true(smp.sig_p$ == 1)) @sys.any;

    expect @enter_exec_st => {@fetch1_assert} @sys.any else
        dut_error("PROTOCOL ERROR");

    assume @enter_exec_st => {@fetch1_assert} @sys.any else {
        dut_error("ASSUMPTION");
        stop_run();
    };
};
