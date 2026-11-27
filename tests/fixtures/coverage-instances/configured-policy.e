unit policy_u {
    event cg;
    !n: uint(bits: 2) = 0;
    !masked: bool = FALSE;
    !probes: uint = 0;

    probe(): bool is { probes = probes + 1; return TRUE; };
    cover cg using per_unit_instance is {
        item n: uint(bits: 2) using
            when = probe(), instance_ignore = inst.masked && n == 1, illegal = n == 1;
    };
    send(value: uint(bits: 2)) is { n = value; emit cg; };
};
extend sys {
    a: policy_u is instance;
    b: policy_u is instance;
    setup() is also {
        set_config(cover, mode, normal, max_int_buckets, 4, at_least, 2, check_illegal_immediately, FALSE);
        a.masked = TRUE;
    };
    run() is also {
        a.send(1);
        b.send(1);
        a.send(0);
        set_config(cover, mode, off);
        a.send(2);
        set_config(cover, mode, count_only);
        a.send(0);
        b.send(2);
        out("probes ", a.probes, " ", b.probes, "\n");
    };
    check() is also { out("check\n"); };
    finalize() is also { out("final\n"); };
};
