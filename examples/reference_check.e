<'
// Deterministic cross-implementation check for e1647-rs.
//
// Every line is labeled and fully deterministic: our interpreter
// (`e1647 run`) and Cadence Specman must print identical text. The two
// outputs are diffed and the result recorded in docs/VERIFICATION.md.
// `int_div` and `bitnot` are probes: they document semantics we had not
// confirmed on a reference simulator before.
//
// Methods live on `sys` on purpose: a bare `var m: math_s;` is an
// unallocated reference in real e (a method call on it crashes Specman),
// and `new` is outside the e1647-rs subset, so both implementations run
// this file with sys methods only.
// Reference run: see docs/VERIFICATION.md (playground share link there).

type mode_e: [IDLE, BUSY];

extend sys {
    add(a: uint, b: uint): uint is {
        return a + b;
    };

    sum_to(n: uint): uint is {
        var total: uint = 0;
        var i: uint = 1;
        while i <= n {
            total = total + i;
            i = i + 1;
        };
        return total;
    };

    run() is also {
        out("arith=", 1 + 2 * 3, "\n");
        out("paren=", (1 + 2) * 3, "\n");
        out("shift=", 1 << 4, " ", 256 >> 3, "\n");
        out("bits=", 12 & 10, " ", 12 | 10, " ", 12 ^ 10, "\n");
        out("lits=", 8'hFF, " ", 0xAB, " ", 0b1010, " ", 0o17, " ", 8'b1010_1010, "\n");
        out("cmp=", 3 < 5, " ", 3 == 3, " ", 3 != 3, "\n");
        out("logic=", TRUE && !FALSE, " ", FALSE || TRUE, "\n");
        out("unary=", -7 + 10, "\n");
        out("mod=", 17 % 5, "\n");
        out("int_div=", 7 / 2, "\n");
        out("bitnot=", ~0, "\n");
        out("call=", add(20, 22), " ", sum_to(6), "\n");
        out("loop=");
        var i: uint = 0;
        while i < 6 {
            i = i + 1;
            if i == 2 {
                continue;
            };
            if i == 5 {
                break;
            };
            out(i, " ");
        };
        out("\n");
        var xs: list of uint;
        xs.push(30);
        xs.push(12);
        xs[1] = 12;
        out("list=", xs.size(), " ", xs[0] + xs[1], "\n");
        var mode: mode_e = BUSY;
        out("enum=", mode == BUSY, "\n");
        out("end=str-check-complete\n");
    };
};
'>
