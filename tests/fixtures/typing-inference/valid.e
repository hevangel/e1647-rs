struct box_s {
    size: uint;
};
struct owner_s {
    input: string;
    copy(input: uint): uint is {
        var n := input;
        var alias := n;
        if TRUE {
            var branch := "shadow";
            var local: string = branch;
        };
        var typed: uint = n;
        return alias;
    };
    inspect(p: box_s, names: list of string) is {
        var alias := p;
        var n := alias.size;
        var first_value := names[0];
        var size: uint = n;
        var label: string = first_value;
    };
    untyped(input) is {
        var unresolved := input;
        unresolved.maybe;
        var mixed := {1; "text"};
        var uncertain: box_s = mixed[0];
        var empty_values := {};
        var item: box_s = empty_values[0];
        var conditional := TRUE ? 1 : "text";
        conditional.maybe;
        var null_ref := NULL;
        null_ref = new box_s;
        var opaque := unknown_routine();
        opaque.maybe;
    };
    separate() is {
        var field_value: string = input;
    };
};
