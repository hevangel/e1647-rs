struct base_s { !id: int; };
struct child_s like base_s { !number: int; };
struct other_s like base_s { !text: string; };
extend sys {
    run() is also {
        var base: base_s = new child_s;
        if base is a child_s (direct) {
            var b01: string = direct.number;
        };
        if (base is a child_s (both)) and both.missing == 0 { };
        if not (base is a child_s (negative)) or negative.missing == 0 { };
        if not (base is a child_s (inverse)) { } else {
            var b02: string = inverse.number;
        };
        while base is a child_s (looped) {
            var b03: string = looped.number;
            break;
        };
        var b04: string = base is a child_s (choice) ? choice.number : 0;
        var implication: bool = (base is a child_s (implied)) => implied.missing == 0;
        if (base is a child_s (joined)) or (base is a child_s (joined)) {
            var b05: string = joined.number;
        };
        if (base is a child_s (replaced)) and (base is a other_s (replaced)) {
            var b06: int = replaced.text;
        };
        check that not (base is a child_s (checked)) else {
            var b07: string = checked.number;
        };
    };
};
