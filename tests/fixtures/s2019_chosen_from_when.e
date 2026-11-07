<'
struct packet_s { x: uint; };
struct base_s {
    kind: bit;
    !lps: list of packet_s;
    p: packet_s;
    when BUSY'kind {
        keep p chosen_from read_only(lps);
    };
};
'>
