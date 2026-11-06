<'
template struct map of (<key'type>: scalar, <value'type> = int, <max_size'exp>: uint = 1000)
    conditions { <max_size'exp> > 0; <max_size'exp> % 100 == 0 } {
    !n: uint;
};
extend sys {
    good: map of (uint, string, 300);
    bad:  map of (uint, string, 150);
};
'>
