struct box_s {
    scale(v: uint): uint is {
        return v * 2;
    };
    run() is {
        var r: uint = scale("wide");
    };
};
