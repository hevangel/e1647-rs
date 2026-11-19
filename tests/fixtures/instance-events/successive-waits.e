extend sys {
    event first_e;
    event second_e;
    worker() @sys.any is { wait @first_e; wait @second_e; out("successive wait ages passed"); };
    driver() @sys.any is {
        wait [600]*cycle;
        emit first_e;
        wait [600]*cycle;
        emit second_e;
    };
    run() is also { start driver(); compute worker(); };
};
