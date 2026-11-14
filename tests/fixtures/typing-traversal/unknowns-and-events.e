struct child_s {
    event tick;
    p: in event_port is instance;
    n: uint;
};
struct owner_s {
    obj: child_s;
    n: uint;
    check() is {
        var casted: uint = unknown().as_a(string);
        var tested: bool = unknown() in [unknown()..unknown()];
        var tbl: bool = (unknown(), n) in_table { unknown(), [0..n]; };
        var evaluated: uint = evaluate (value) { value = unknown(); };
        var created: child_s = new with { n = 1; };
        emit obj.tick;
        emit obj.p;
        for each (item) using index (idx) in unknown_list { print item, idx; };
    };
};
