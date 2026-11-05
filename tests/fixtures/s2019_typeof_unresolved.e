<'
-- 5.9: typeof() resolves a field path, a local var, a method parameter
-- or a literal. `u` names no visible field, so its static type cannot
-- be determined and E0208 reports at the typeof type position.
struct holder {
    a: int;
    b: typeof(u);
};
'>
