unit base_u {
    event bad;
    !failures: uint = 0;
    expect not @bad else { failures = failures + 1; };
    fire() is { emit bad; };
};
unit derived_u like base_u {};
extend sys {
    a: derived_u is instance;
    b: derived_u is instance;
    run() is also {
        var aliases := { a; a; b; };
        aliases[0].fire();
        wait [2]*cycle;
        check that a.failures == 1;
        check that b.failures == 0;
        out("inherited rule owners passed");
    };
};
