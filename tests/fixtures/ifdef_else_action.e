extend sys {
    run() is also {
        #ifdef MISSING {
            out("no");
        } #else {
            out("yes");
        };
    };
};
