extend sys {
    run() is also {
        hdl_load("PROFILE_DESIGN", "profile", "xezim");
        hdl_run(1);
        hdl_set("profile.word", -2147483648);
        hdl_run(1);
        out("min ", hdl_get("profile.word"), "\n");
        'profile.word' = 2147483647;
        hdl_run(1);
        out("max ", 'profile.word', "\n");
        for i from 1 to 3 {
            'profile.clk' = 1;
            hdl_run(1);
            out("edge ", i, " count ", 'profile.count', "\n");
            'profile.clk' = 0;
            hdl_run(1);
        };
        hdl_run(0);
        hdl_finish();
        hdl_finish();
        out("finished\n");
    };
};
