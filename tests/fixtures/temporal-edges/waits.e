extend sys {
    !level: int = 10;
    driver() @sys.any is {
        level = 20;
        sync;
        level = 5;
        sync;
        level = 6;
    };
    run() is also {
        start driver();
        wait rise(level);
        out("up\n");
        wait fall(level);
        out("down\n");
    };
};
