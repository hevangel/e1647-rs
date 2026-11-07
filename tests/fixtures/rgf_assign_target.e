<'
struct plain {
    x: uint;
};
extend sys {
    run() is also {
        var p: plain;
        p.x rgf= 3;
        var q: uint = 1;
        q rgf= 4;
        out(rgf_was_generated(p.x));
        out(rgf_get_range(p.x));
    };
};
'>
