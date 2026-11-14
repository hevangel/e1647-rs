struct base_s {
    static flag: bool;
};
struct child_s like base_s {
    static accept(): uint is { return flag; };
};
