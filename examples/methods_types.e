// A small tour of the supported subset: enumeral type declarations,
// enum-typed fields, soft constraints, methods with statement bodies,
// calls, indexing, `gen`, `print`, `when` subtype members and `while`
// loops.
//
// Parse it with: cargo run -- parse examples/methods_types.e
//
// Parsing is not simulation: this file only demonstrates what the
// front-end accepts today.

type color_e: [BLACK = 0, RED = 1, GREEN = 2];

struct pixel_s {
    color: color_e;
    intensity: uint(bits: 8);
    keep intensity < 256;
    keep soft color == RED;

    gray(): uint(bits: 8) is {
        return intensity;
    };

    scale(factor: uint): uint is {
        var scaled: uint = intensity * factor;
        if scaled > 255 {
            scaled = 255;
        };
        return scaled;
    };
};

extend pixel_s {
    darken() is also {
        intensity = intensity / 2;
    };
};

struct frame_s {
    pixels: list of pixel_s;
    keep pixels.size() > 0;
    pick(i: uint): pixel_s is {
        return pixels[i];
    };
};

type mode_e: [IDLE, BUSY];

struct channel_s {
    mode: mode_e;
    fill: uint(bits: 8);

    when BUSY'kind channel_s {
        payload: list of uint(bits: 8);
        keep fill < 200;
    };

    drain() is {
        var i: uint = 0;
        while i < 4 {
            i = i + 1;
        };
    };
};

// The entry point: with the file run (`e1647 run --seed 1
// examples/methods_types.e`), the built-in sys unit's run() executes.
extend sys {
    run() is also {
        var p: pixel_s;
        gen p;
        print p.intensity;
        print p.gray();
        print p.scale(2);
        var f: frame_s;
        gen f;
        print f.pick(0).intensity;
        out("methods_types.e ran to completion\n");
    };
};
