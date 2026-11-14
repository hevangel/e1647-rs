unit owner_u {
    event cg;
    flag: bool;
    cover cg using when = 7 is {
        item flag: bool using when = 7, when = flag, ignore = "bad", ignore = flag;
    };
};
extend owner_u {
    cover cg using when = TRUE is also { };
};
