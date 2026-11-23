define OFFSET 2;
type small_t: int(bits: 8) [-OFFSET..OFFSET + 2];
type small_alias_t: small_t;
extend sys {
    run() is also {
        print set_of_values(small_t), full_set_of_values(small_t);
        print set_of_values(small_alias_t), full_set_of_values(small_alias_t);
        print (5) in_table { set_of_values(small_alias_t); };
        print (5) in_table { full_set_of_values(small_alias_t); };
        print (4) in_table { set_of_values(small_alias_t); };
    };
};
