struct origin_s {
    static event tick;
    static flag: uint;
};
struct observer_s {
    static flag: bool;
    on origin_s::tick { var a: uint = flag; };
};
struct child_s like observer_s { };
