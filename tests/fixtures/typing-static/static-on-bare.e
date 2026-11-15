struct owner_s {
    static event tick;
    static flag: bool;
    static on tick { var a: uint = flag; };
};
struct child_s like owner_s { };
