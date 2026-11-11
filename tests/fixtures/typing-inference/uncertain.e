type cycle_a: cycle_b;
type cycle_b: cycle_a;
struct numeric_s implementing numeric_i of (numeric_s, TRUE, 32) {
};
numeric_type custom_t: numeric_s;
struct owner_s {
    work(custom: custom_t, cyclic: cycle_a, unresolved) is {
        var a := custom + 1;
        var b := unresolved + 1;
        var c := unresolved * 1.0;
        var d := unresolved & 1;
        var e := -unresolved;
        var f := ~unresolved;
        var unknown_a: string = a;
        var unknown_b: string = b;
        var unknown_c: string = c;
        var unknown_d: string = d;
        var unknown_e: string = e;
        var unknown_f: string = f;
        cyclic.maybe;
    };
};
