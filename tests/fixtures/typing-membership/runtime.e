type tone_t: [low_t, middle_t, high_t];
struct item_s { !n: uint; };
extend sys {
    !calls: uint = 0;
    !left_calls: uint = 0;
    source(): list of uint is { calls = calls + 1; return { 3; 1; 2; 1; }; };
    left(): list of uint is { left_calls = left_calls + 1; return { 1; 1; }; };
    run() is also {
        var xs: list of uint = { 3; 1; 2; 1; };
        var ys: list of uint = { 1; 1; 3; };
        check that ys in xs;
        check that not ({ 1; 1; 1; } in xs);
        check that { 2; 1; } in xs;
        check that { 4; } not in xs;
        check that {} in xs;
        check that {} in {};
        check that not (1 in {});
        check that not ({ 1; } in {});
        check that xs in xs;
        check that { 1; 1; 1; } in [1,2];
        check that { 1; 1; 1; } in [1..2];
        check that { 1; 4; } not in [1..2];
        var s: set = [1..3];
        check that { 2; 2; 3; } in s;
        check that { 0; 2; } not in s;
        check that { "b"; "a"; "b"; } in { "a"; "b"; "b"; };
        check that { "b"; "b"; } not in { "a"; "b"; };
        check that { TRUE; FALSE; } in { FALSE; TRUE; };
        check that { TRUE; TRUE; } not in { FALSE; TRUE; };
        check that { TRUE; TRUE; } in set_of_values(bool);
        check that { low_t; high_t; } in { high_t; low_t; middle_t; };
        check that low_t in [low_t..high_t];
        check that { low_t; high_t; } in set_of_values(tone_t);
        check that middle_t in set_of_values(tone_t);
        var p: item_s = new;
        var q: item_s = new;
        check that { p; p; } in { p; q; p; };
        check that { p; p; } not in { p; q; };
        check that NULL in { p; NULL; };
        check that { NULL; NULL; } not in { p; NULL; };
        check that left() in source();
        check that calls == 1;
        check that left_calls == 1;
        check that xs.size() == 4;
        check that xs[0] == 3;
        check that { 1.0; 2.0; } in { 2; 1; };
        check that 2.0 in { 1; 2; };
        var words: list of string = { "ok"; };
        var selected: string = words[evaluate { value = 0; }];
        check that selected == "ok";
        words[evaluate { value = 0; }] = "changed";
        check that words[0] == "changed";
        var inferred_set := [1..2];
        inferred_set = evaluate { value = [3..4]; };
        check that 3 in inferred_set;
        // The first real matches either integer. Preserve the exact
        // integer's only match instead of committing greedily.
        check that { 9007199254740992.0; 9007199254740992; } in
            { 9007199254740992; 9007199254740993; };
        check that { 9007199254740992; 9007199254740992; } not in
            { 9007199254740992; 9007199254740993; };
        out("membership checks passed");
    };
};
