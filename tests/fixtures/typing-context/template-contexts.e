template struct box_t of <type> { value: <type>; };
struct owner_s {
    check() is {
        var box: box_t of (uint) = new with { value = "bad"; };
    };
};
