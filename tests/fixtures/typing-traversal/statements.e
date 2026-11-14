struct child_s {
    n: uint;
    task(v: uint) @tick is { };
};
struct owner_s {
    label: string;
    ~n: uint;
    obj: child_s;
    task(v: uint) @tick is { };
    check() is {
        print label * 2;
        gen obj.absent;
        wait [label * 2];
        emit obj.absent;
        start task(label * 2);
        compute obj.task(label * 2);
        n rgf= "bad";
    };
};
