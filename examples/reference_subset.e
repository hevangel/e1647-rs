// Independently authored e example used as the Specman reference run for
// hevangel/e1647-rs (issue #1). It exercises exactly the subset our parser
// implements: struct/unit/extend declarations with `like`, scalar fields
// with `uint(bits: N)` widths, nibble, `list of`, `keep` constraint
// expressions, `TRUE`/`FALSE`-free simple literals, and sized/unsized based
// literals in expressions (`8'hFF`, `0b1010`, `0o17`).
//
// Reference runs: the original save is
// https://www.edaplayground.com/x/7Jrh (previous account; fails with
// xmelab *F,NOTOP because its Design pane is empty). The working copy with
// a minimal `module design` top and the successful Specman 2025.03 /
// Xcelium 25.03 run of 2026-10-27 is
// https://www.edaplayground.com/x/kvcS — see docs/VERIFICATION.md.
<'
struct packet_s {
    addr: uint(bits: 8);
    data: uint(bits: 8);
    tag: nibble;
    par: list of uint(bits: 4);
    keep addr < 128;
    keep data != 0;
};

extend packet_s {
    keep addr > 0;
};

struct frame_s like packet_s {
    crc: uint(bits: 32);
};

extend sys {
    run() is also {
        var p: packet_s;
        gen p;
        print p;
        out("based literals: 8'hFF=", 8'hFF, " 0b1010=", 0b1010, " 0o17=", 0o17);
        out("e1647-rs subset constructs accepted by Specman");
    };
};
'>
