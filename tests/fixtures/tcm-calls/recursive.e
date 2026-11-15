extend sys {
    descend(n: uint) is {
        var saved: uint = n;
        if n > 0 {
            descend(n - 1);
        };
        sync;
        out(saved, "\n");
        return;
        out("unreachable\n");
    };
    descend(n: uint) is also {
        sync;
        out("also ", n, "\n");
    };
    run() is also {
        for i from 1 to 2 {
            compute descend(i);
        };
        out("done\n");
    };
};
