struct consumer_s {
    !gate: semaphore;
    take(n: uint): uint is { gate.down(); sync; return n + 10; };
};
extend sys {
    !h: consumer_s;
    !calls: uint = 0;
    argument(): uint is { calls = calls + 1; return calls; };
    consumer(n: uint) @sys.any is {
        var answer: uint = h.take(n + argument());
        out("consumer ", n, " answer ", answer, "\n");
    };
    producer() @sys.any is { wait [2]*cycle; h.gate.up(); h.gate.up(); };
    run() is also {
        h = new consumer_s;
        h.gate = new semaphore;
        start consumer(1);
        start consumer(2);
        start producer();
    };
};
