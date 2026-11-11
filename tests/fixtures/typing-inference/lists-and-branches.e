struct owner_s {
    run() is {
        var names := {"a"; "b"};
        var wrong: uint = names[0];
        var label := TRUE ? "a" : "b";
        label = 3;
        var values := {1; 2};
        values = {"bad"};
        var nested := {"a"; 1 * "bad"};
        var conditional := TRUE ? 1 * "bad" : 0;
    };
};
