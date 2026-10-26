// Independently authored e example used as the Specman reference run for
// hevangel/e1647-rs (issue #1). It exercises exactly the subset our parser
// implements: struct/unit/extend declarations with `like`, scalar fields
// with `uint(bits: N)` widths, nibble, `list of`, `keep` constraint
// expressions, `TRUE`/`FALSE`-free simple literals, and sized/unsized based
// literals in expressions (`8'hFF`, `0b1010`, `0o17`).
//
// Saved on EDA Playground at https://www.edaplayground.com/x/7Jrh
// (language "Specman e + SV/Verilog", simulator Cadence Xcelium 25.03 with
// Specman 2025.03). Run output is recorded in docs/VERIFICATION.md.
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
