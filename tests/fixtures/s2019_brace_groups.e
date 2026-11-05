<'
{
    type color_t: [RED, GREEN];
    struct pkt {
        { a: uint; b: uint; }
        { keep a < 10; }
        c: color_t;
    };
}
extend sys {
    run() is also {
        var p: pkt;
        gen p;
        out("ok");
    };
};
'>
