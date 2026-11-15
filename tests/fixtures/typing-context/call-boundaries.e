struct owner_s {
    accept(v: uint): string is { return v.to_string(); };
    static accept_static(v: uint): string is { return v.to_string(); };
    check() is {
        var a: string = accept(evaluate { value = 1; });
        var b: string = owner_s::accept_static(evaluate { value = 2; });
        var c: string = unknown(evaluate { value = 3; });
        var d: uint = evaluate { var nested := evaluate { value = "x"; }; value = 4; };
    };
};
