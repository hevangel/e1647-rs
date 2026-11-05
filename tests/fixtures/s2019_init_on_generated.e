// Clause 6.9.1: an init value is not allowed on a generated field —
// `depth` has no `!` (and no `const`), so its value is produced by `gen`
// and a written initial value would never survive (E0211).
struct cfg {
    depth: uint = 16;
};
