extend sys {
    cover cg is {
        item n: bool = FALSE;
        item m: bool = FALSE;
        cross n, m using name = "pa" + "ir";
        transition n using name = ("pair");
    };
};
