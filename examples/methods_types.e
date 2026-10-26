// A small tour of the supported subset: enumeral type declarations,
// enum-typed fields, soft constraints, methods with statement bodies,
// calls, indexing, `gen` and `print`.
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
    pick(i: uint): pixel_s is {
        return pixels[i];
    };
};
