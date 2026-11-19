extend sys {
    event cg;
    cover cg is { item n: uint using ranges = {
        range([0..65535], "", 1);
        range([70000]);
    }; };
};
