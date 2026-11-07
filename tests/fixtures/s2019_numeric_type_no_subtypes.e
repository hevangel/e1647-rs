<'
struct fx_s implementing numeric_i of (fx_s, TRUE, 32) {
    !raw: int;
    when BUSY'kind { !x: int; };
};
numeric_type fx_t: fx_s;
type narrow: fx_t [0..5];
struct sub like fx_s { x: int; };
'>
