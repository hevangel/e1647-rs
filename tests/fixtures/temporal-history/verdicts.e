extend sys {
    !flag: bool = TRUE;
    expect not {true(flag); true(not flag)} else out("invariant violation\n");
    expect {true(flag); true(flag)} else out("unsatisfied\n");
    run() is also {
        wait [1]*cycle;
        flag = FALSE;
        wait [1]*cycle;
        out("done\n");
    };
};
