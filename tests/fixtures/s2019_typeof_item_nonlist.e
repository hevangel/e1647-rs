<'
-- 5.9: typeof_item() requires a list-typed argument. `a` is an int, so
-- the static type is not a list and E0209 reports at the typeof_item
-- type position.
struct holder {
    a: int;
    b: typeof_item(a);
};
'>
