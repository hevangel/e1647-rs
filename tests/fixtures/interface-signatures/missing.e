interface required_i { missing(): int; };
struct missing_s implementing required_i { };
extend sys {
    run() is also {
        var target: required_i = new missing_s;
        var result: int = target.missing();
    };
};
