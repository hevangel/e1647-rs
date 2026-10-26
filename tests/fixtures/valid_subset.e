// Independently authored valid example: struct, extend and like subset.
struct packet {
    addr: uint(bits: 8);
    data: uint(bits: 8);
    keep addr < 128;
    keep data != 0;
    par: list of uint(bits: 4);
};

extend packet {
    keep addr > 0;
};

struct frame like packet {
    crc: uint(bits: 32);
};
