extend sys {
    event cg;
    cover cg is {
        item n: int using ranges = {
            range([-170141183460469231731687303715884105727 - 1..170141183460469231731687303715884105727], "", 1);
        };
    };
};
