struct item_s {
    score: uint;
};
extend sys {
    run() is also {
        var xs: list of item_s;
        var it: string = "outer";
        xs.max(it.absent);
        xs.min(it.score * "bad");
        var wrong: int = xs.max(it.score);
        var index: string = xs.min_index(it.score);
        var still_string: string = it;
    };
};
