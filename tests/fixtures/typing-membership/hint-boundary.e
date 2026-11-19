extend sys {
    run() is also {
        var ok: bool = evaluate { value = 1; } in { 1; 2; };
    };
};
