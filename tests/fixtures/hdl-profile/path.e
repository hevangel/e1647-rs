extend sys {
    run() is also {
        hdl_load("PROFILE_DESIGN", "profile");
        hdl_set("profile.word\nFINISH", 1);
    };
};
