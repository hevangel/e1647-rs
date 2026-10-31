extend sys {
    run() is also {
        #ifdef SAVE_THE_RUN {
            this is deliberately not e at all !!!
        }; // #ifdef SAVE_THE_RUN
        out("reached");
    };
};
