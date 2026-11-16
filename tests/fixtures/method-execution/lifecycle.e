struct audit_s { static !trace: string = ""; static !setup_value: int = 0; };
extend any_unit {
    run() is first { audit_s::trace = append(audit_s::trace, "F"); };
    run() is also { audit_s::trace = append(audit_s::trace, "A"); };
};
struct constructor_s {
    !value: int = 0;
    init() is also { value = 1; };
    init() is first { value = 2; return; };
    init() is also { value = value + 10; };
};
unit dut_u {
    run() is only { audit_s::trace = append(audit_s::trace, "D"); };
    run() is also { audit_s::trace = append(audit_s::trace, "X"); };
};
extend sys {
    !dut: dut_u;
    !constructed: constructor_s;
    setup() is also { audit_s::setup_value = -1; };
    setup() is first { audit_s::setup_value = 10; return; };
    setup() is also { audit_s::setup_value = audit_s::setup_value + 1; };
    run() is only {
        check that audit_s::setup_value == 11;
        check that constructed.value == 12;
        audit_s::trace = append(audit_s::trace, "S");
    };
    check() is also {
        check that audit_s::trace == "FSAFDXA";
        out("lifecycle wrappers passed\n");
    };
};
