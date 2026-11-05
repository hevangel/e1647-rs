// Clause 6.9.1: `!` (do-not-generate) and `~` (range-generated) are
// mutually exclusive prefixes (E0213).
struct s {
    !~x: uint;
};
