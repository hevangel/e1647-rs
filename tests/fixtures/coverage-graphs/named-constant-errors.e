extend sys {
    const cycle_a: int = cycle_b;
    const cycle_b: int = cycle_a;
    !mutable_bound: int = 1;
    !runtime_width: int = 8; const sized: uint(bits: runtime_width) = 1;
    const divided: int = 1 / 0;
    !MAX_INT: int = 2;
    cover cg is {
        item a: uint = 0 using ranges = { range([0..cycle_a], ""); };
        item b: uint = 0 using ranges = { range([0..mutable_bound], ""); };
        item c: uint = 0 using at_least = sized;
        item d: uint = 0 using weight = divided;
        item e: uint = 0 using ranges = { range([0..MAX_INT], ""); };
    };
};
