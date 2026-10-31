type addr_t : uint (bits : 16);
struct pkt {
    address : addr_t;
};
extend pkt {
    keep soft address == select {
        50 : min;
        25 : max;
        25 : others;
    };
    keep soft address == select {
        100 : normal(MAX_UINT/2, MAX_UINT/10);
    };
    keep soft address == select {
        50 : normal(2, 10);
        50 : edges;
    };
    keep soft address == select {
        10 : [1, 2, 3];
        90 : others;
    };
};
