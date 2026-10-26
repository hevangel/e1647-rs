struct packet {
    addr: uint(bits: 8);
};

struct derived like base_model {
    crc: uint(bits: 32);
};
