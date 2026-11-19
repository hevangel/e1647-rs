extend sys {
    event cg;
    cover cg is {
        item n: int using ranges = {
            range([0..4], "", UNDEF, -1);
        };
    };
};
