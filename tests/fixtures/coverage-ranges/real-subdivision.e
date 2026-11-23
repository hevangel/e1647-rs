type measure_t: real;
struct dimensions_s {
    static const half_step: measure_t = 0.125;
};
extend sys {
    event cg;
    !n: measure_t = 0.0;
    const low: measure_t = 0.5 - 0.5;
    const high: measure_t = 0.1 + 0.5;
    cover cg is {
        item n: measure_t using ranges = {
            range([low..high], "", dimensions_s::half_step * 2, 2);
        };
    };
    setup() is also { set_config(cover, mode, normal); };
    run() is also {
        emit cg;
        n = 0.25; emit cg;
        n = 0.5; emit cg;
        n = 0.6; emit cg;
    };
};
