extend sys {
    cover cg is {
        item n: bool = FALSE;
        item m: bool = FALSE;
        cross n, m using name = 7;
        transition n using name = "prefix" + 7;
        cross n, m using name = missing;
        transition m using name = "prefix" "tail";
        cross n, m using name = "prefix" +;
        transition n using name =, weight = 2;
    };
};
