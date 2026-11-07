<'
struct packet { x: uint; };
template struct map of (<key'type>: scalar, <value'type> = int) {
    !n: uint;
};
extend sys {
    good: map of (uint, string);
    bad:  map of (packet, string);
};
'>
