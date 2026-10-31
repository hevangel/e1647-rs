extend sys {
    ev: event;
    clk: event;
    done: uint;

    expect @ev => eventually true(done > 0) @clk else
        dut_error("NO DONE");

    assume @ev => {@clk; @clk} @clk else {
        dut_error("NO TWO CYCLES");
        stop_run();
    };

    run() is also {
        emit ev;
        wait until @ev;
        wait [2]*@ev;
        wait true(done == 0) @clk;
        wait {@clk; @clk};
        sync;
        sync @clk;
    };

    check_it() @clk is {
        sync cycle;
        while done < 3 {
            wait [1]*cycle;
            done = done + 1;
        };
    };
};
