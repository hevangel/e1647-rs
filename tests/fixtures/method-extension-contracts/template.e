template struct box_t of <type> {
    f(x: <type>) is { };
};
template extend box_t of <type> {
    f(x: string) is also { };
};
extend sys {
    !good: box_t of string;
    !bad: box_t of int;
};
