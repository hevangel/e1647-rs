struct base_s { !id: int = 4; };
struct child_s like base_s { !number: int = 7; };
struct other_s like base_s { !text: string = "other"; };
extend sys {
    run() is also {
        var base: base_s = new child_s;
        var count: int = 0;
        if (base is a child_s (direct)) and direct.number == 7 {
            check that direct.id == 4;
            count = count + 1;
        };
        if not (base is a child_s (inverse)) { check that FALSE; } else {
            check that inverse.number == 7;
            count = count + 1;
        };
        if not (base is a child_s (right)) or right.number == 7 {
            count = count + 1;
        };
        while base is a child_s (looped) {
            check that looped.number == 7;
            count = count + 1;
            break;
        };
        var chosen: int = base is a child_s (choice) ? choice.number : 0;
        check that chosen == 7;
        var implication: bool = (base is a child_s (implied)) => implied.number == 7;
        check that implication;
        if (base is a child_s (joined)) or (base is a child_s (joined)) {
            check that joined.number == 7;
            count = count + 1;
        };
        check that count == 5;
        out("guarded bindings passed");
    };
};
