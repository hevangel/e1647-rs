extend sys {
    event cg;
    cover cg is {
        item n: int using ranges = {
            range([0..65536], "", 1);
        };
    };
};
