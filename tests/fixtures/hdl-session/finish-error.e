extend sys {
    run() is also {
        out("before\n");
        hdl_finish();
        out("unreachable\n");
    };
};
