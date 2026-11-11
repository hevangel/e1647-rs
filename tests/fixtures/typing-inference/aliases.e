type word_t: uint;
type alias_t: word_t;
type names_t: list of string;
struct owner_s {
    work(v: alias_t, names: names_t): uint is {
        var n := v + 1;
        var label := names[0];
        var wrong: uint = label;
        return n;
    };
};
