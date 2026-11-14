type count_t: uint;
struct child_s { count: uint; };
struct base_s {
    static count: count_t;
};
struct api_s like base_s {
    static event tick;
    static !flag: bool = TRUE;
    static box: child_s;
    static xs: list of uint;
    static add(n: uint): uint is { count = count + n; return count; };
    static build(): uint is {
        var flag: uint = 3;
        var child := new child_s with { count = 7; };
        var value := evaluate uint { value = child.count + count; };
        return value;
    };
    static shadow(count: uint): uint is { count = count + 1; return count; };
    static call_observer(n: uint): uint is {
        observer_s::add(n);
        count = count + 1;
        return count;
    };
    static on tick { count = count + 1; };
};
struct observer_s {
    static count: uint;
    static add(n: uint) is { count = count + n; };
    on api_s::tick { count = count + 2; };
};
extend api_s {
    static add(value) is also { api_s::box.count = value; };
};
extend sys {
    run() is also {
        var a: uint = api_s::add(2);
        check that a == 2;
        check that api_s::box.count == 2;
        emit api_s::tick;
        check that base_s::count == 3;
        check that api_s::build() == 10;
        check that observer_s::count == 2;
        check that api_s::call_observer(4) == 4;
        check that observer_s::count == 6;
        check that api_s::shadow(9) == 10;
        check that base_s::count == 4;
        api_s::xs = { 1; 2; };
        check that api_s::xs[1] == 2;
        out("static scopes passed");
    };
};
