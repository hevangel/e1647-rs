extend sys {
    run() is also {
        hdl_load("PROFILE_DESIGN", "profile");
        hdl_run(1);
        out(hdl_get("profile.absent"));
    };
};
