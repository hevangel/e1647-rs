extend sys {
    event cg;
    cover cg is {
        item n: string using ranges = { range([0..4]); };
    };
};
