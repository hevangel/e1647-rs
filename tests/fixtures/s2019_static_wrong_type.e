<'
struct a {
    static s: uint;
};
struct b like a {
};
extend sys {
    run() is also {
        out(b::s);
    };
};
'>
