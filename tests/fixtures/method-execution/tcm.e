struct worker_s {
    !trace: string = "";
    !started_trace: string = "";
    !good: int = 0;
    !bad: int = 0;
    work(x: int): int @sys.any is { sync; check that x == 5; x = 99; result = 10; trace = append(trace, "C"); };
    work(renamed) is also { sync; check that renamed == 5; check that result == 10; result = result + 2; trace = append(trace, "A"); };
    work(renamed) is first { sync; renamed = 5; trace = append(trace, "F"); };
    launch(x: int) @sys.any is { sync; check that x == 5; x = 99; started_trace = append(started_trace, "C"); };
    launch(renamed) is also { sync; check that renamed == 5; started_trace = append(started_trace, "A"); };
    launch(renamed) is first { sync; renamed = 5; started_trace = append(started_trace, "F"); };
    early_start() @sys.any is { bad = bad + 1; };
    early_start() is first { sync; good = good + 1; return; bad = bad + 1; };
    early_start() is also { sync; good = good + 1; };
    early_value(x: int): int @sys.any is { bad = bad + 1; return -1; };
    early_value(x) is first { sync; x = 7; result = 50; return; };
    early_value(x) is also { sync; check that x == 2; check that result == 50; result = result + 3; };
};
extend sys {
    !ready: bool = FALSE;
    run() is also {
        var worker: worker_s = new worker_s;
        check that ready;
        check that worker.work(1) == 12;
        check that worker.trace == "FCA";
        check that worker.early_value(2) == 53;
        start worker.work(2);
        start worker.launch(1);
        start worker.early_start();
        wait [10]*cycle;
        check that worker.started_trace == "FCA";
        check that worker.trace == "FCAFCA";
        check that worker.good == 2;
        check that worker.bad == 0;
    };
    run() is first { sync; ready = TRUE; };
    run() is also { check that ready; out("suspended wrappers passed\n"); };
};
