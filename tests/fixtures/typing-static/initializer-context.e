struct owner_s {
    flag: bool;
    static !count: uint = flag;
    static !label: string = owner_s::count;
};
