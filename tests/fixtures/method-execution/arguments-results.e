struct worker_s {
    static !created: int = 0;
    !result: string = "field";
    !discarded: int = 0;
    init() is also { worker_s::created = worker_s::created + 1; };
    adjust(x: int): int is { check that x == 6; x = 99; result = 20; };
    adjust(renamed) is also { check that renamed == 6; check that result == 20; result = result + 2; };
    adjust(renamed) is first { renamed = 6; result = 777; };
    early(x: int): int is { discarded = discarded + 1; return -1; };
    early(x) is also { discarded = discarded + 1; };
    early(x) is first { result = 40; return; discarded = discarded + 1; };
    early(x) is also { check that x == 2; check that result == 40; result = result + 2; };
    after(): int is { return 3; };
    after() is also { result = result + 1; return; discarded = discarded + 1; };
    after() is also { check that result == 4; result = result + 1; };
    choose(): worker_s is { return new worker_s; };
    choose() is also { return NULL; };
};
extend sys {
    run() is also {
        var worker: worker_s = new worker_s;
        check that worker.adjust(2) == 22;
        check that worker.result == "field";
        check that worker.early(2) == 42;
        check that worker.after() == 5;
        check that worker.discarded == 0;
        var baseline: int = worker_s::created;
        var chosen: worker_s = worker.choose();
        check that chosen == NULL;
        check that worker_s::created == baseline + 1;
        out("arguments and results passed\n");
    };
};
