// Clause 6.9.4: a range-generated (`~`) field cannot be const — a const
// field holds one initialised value, while a `~` field's reduced set is
// produced per generation (E0214).
struct s {
    const ~c: uint;
};
