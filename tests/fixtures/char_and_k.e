struct packet {
    a: uint(bits: 8);
    keep a < 32K;
    keep a == 0c"a";
};
