define <e_util_min_value'exp> "MIN_VALUE[ ]\(<type>\)" as {
    set_of_values(<type>).uint_min().as_a(<type>);
};
define <e_util_type_range_dac'exp> "RANGE_SIZE[ ]\(<type>\)" as computed {
    if the_type is a rf_numeric (rfn) {
        return set.uint_size().to_string();
    } else {
        reject_match();
    };
};
define <plain_macro> "NO_KIND[ ]" as {
    out("plain");
};
type addr_t : int (bits : 8);
