extend sys {
    !gate: semaphore;
    take(): uint is { gate.down(); return 1; };
    run() is also {
        gate = new semaphore;
        var answer: uint = take();
    };
};
