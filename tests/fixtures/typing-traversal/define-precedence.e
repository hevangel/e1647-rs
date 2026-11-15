define flag 1;
struct child_s {
    flag: bool;
    count: uint;
};
struct owner_s {
    flag: string;
    MAX_INT: bool;
    make() is {
        var expected: uint = flag;
        var max: uint = MAX_INT;
        var obj := new child_s with { count = flag; };
        check that expected == 1;
        check that max == 2147483647;
        check that obj.count == 1;
    };
};
extend sys {
    obj: owner_s;
    run() is also { obj.make(); out("define scopes passed"); };
};
