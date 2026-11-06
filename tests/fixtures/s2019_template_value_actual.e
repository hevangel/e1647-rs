<'
type width: uint;
template struct box of (<t'type>, <n'exp>: uint) {
    !v: <t'type>;
    size(): uint is { return <n'exp>; };
};
extend sys {
    ok:  box of (int, 3);
    bad: box of (int, width);
};
'>
