struct owner_s {
    check() is {
        var n := 0;
        n = evaluate { value = "bad"; };
        var xs := { 1; 2; };
        xs[0] = evaluate { value = "bad"; };
    };
};
