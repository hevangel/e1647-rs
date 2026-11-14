struct child_s { n: uint; };
struct owner_s {
    event tick;
    label: string;
    obj: child_s;
    expect true(label * 2 > 0) else { print obj.absent; };
    assume ~[label * 2]*@tick;
    check() @tick is {
        wait true(obj.absent > 0);
        sync true(label * 2 > 0);
        wait [label * 2]*@tick;
    };
};
