struct child_s { n: uint; };
struct owner_s {
    event tick;
    label: string;
    obj: child_s;
    expect (change(label * 2) or fail fall(obj.absent)) @tick;
    expect {true(label * 2 > 0); rise(obj.absent)} => eventually true(label * 2 > 0);
    assume [label * 2..obj.absent]*@tick;
};
