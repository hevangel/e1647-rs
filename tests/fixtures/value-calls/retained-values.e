extend sys {
    !data: uint = 1;
    !choose: bool = TRUE;
    pending(): uint is { sync; return 1; };
    unexpected(): uint is { out("wrong branch\n"); return 99; };
    mutate() @sys.any is { data = 100; choose = FALSE; };
    run() is also {
        start mutate();
        var answer: uint = data + (choose ? pending() : unexpected());
        out(answer, " data ", data, " choose ", choose, "\n");
    };
};
