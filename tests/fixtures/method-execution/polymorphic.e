type mode_t: [red_mode, blue_mode];
struct packet_s {
    !mode: mode_t = red_mode;
    !trace: string = "";
    f() is { trace = append(trace, "C"); };
    f() is first { trace = append(trace, "F"); };
    f() is also { trace = append(trace, "A"); };
    when red_mode'mode {
        f() is only { trace = append(trace, "R"); };
        f() is also { trace = append(trace, "X"); };
    };
};
template struct base_t of <type> {
    !trace: string = "";
    f(x: <type>): <type> is { trace = append(trace, "C"); return x; };
    f(x) is first { trace = append(trace, "F"); };
    f(x) is also { trace = append(trace, "A"); };
};
template struct child_t of <type> like base_t of <type> {
    f(x) is only { trace = append(trace, "O"); return x; };
    f(x) is also { trace = append(trace, "X"); };
};
extend sys {
    run() is also {
        var p: red_mode packet_s = new red_mode packet_s;
        p.f();
        check that p.trace == "FRXA";
        var integers: child_t of int = new child_t of int;
        check that integers.f(5) == 5;
        check that integers.trace == "FOXA";
        var strings: child_t of string = new child_t of string;
        check that strings.f("hello") == "hello";
        check that strings.trace == "FOXA";
        out("polymorphic wrappers passed\n");
    };
};
