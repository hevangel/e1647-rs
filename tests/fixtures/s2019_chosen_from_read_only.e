<'
struct packet_s { x: uint; };
struct holder {
    !lps: list of packet_s;
    p: packet_s;
    keep p chosen_from read_only(lps);
    keep p.x > 5;
};
'>
