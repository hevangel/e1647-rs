struct item_s {
    score: uint;
    tag: string;
};
extend sys {
    !calls: uint = 0;
    !xs: list of item_s;
    key(v: item_s): uint is {
        calls += 1;
        xs.clear();
        return v.score;
    };
    run() is also {
        var a: item_s = new item_s;
        var b: item_s = new item_s;
        var c: item_s = new item_s;
        a.score = 2; a.tag = "z";
        b.score = 7; b.tag = "a";
        c.score = 7; c.tag = "b";
        xs.push(a); xs.push(b); xs.push(c);
        var winner: item_s = xs.max(it.score);
        out(winner == c, " ", xs.max_index(it.score), "\n");
        winner.score = 8;
        out(c.score, " ", xs.min(it.tag) == b, "\n");
        out(xs.max(key(it)) == c, " ", calls, " ", xs.size(), "\n");
    };
};
