struct packet {
    addr: uint(bits: 8);
    assert addr_never_zero: addr != 0;
};
