extend sys {
    count(n: uint): uint is {
        if n == 0 { sync; return 1; };
        return count(n - 1) + 1;
    };
    count(n: uint) is also { sync; };
    contextual(): uint is { sync; return evaluate { value = 9; }; };
    run() is also {
        var answer: uint = count(3) + contextual();
        out(answer, "\n");
    };
};
