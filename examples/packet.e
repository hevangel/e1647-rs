// Illustrative e-language-like example for lexer development only.
// Parsing and execution are not implemented yet.
struct packet {
    addr: uint(bits: 8);
    data: uint(bits: 8);
    keep addr < 128;
    keep data != 0;
};
