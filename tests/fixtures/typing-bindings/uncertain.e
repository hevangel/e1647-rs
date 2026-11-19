struct base_s { !id: int; };
struct child_s like base_s { !number: int; };
struct other_s like base_s { !text: string; };
extend sys {
    run() is also {
        var base: base_s;
        if (base is a child_s (optional)) or TRUE {
            var maybe01: string = optional.number;
        };
        if (base is a child_s (different)) or (base is a other_s (different)) {
            var maybe02: string = different.number;
        };
        if base is a child_s (inside) { } else {
            var maybe03: string = inside.number;
        };
        var maybe04: string = inside.number;
        if base is not a child_s (unbound) {
            var maybe05: string = unbound.number;
        };
        var shadowed: string = "original";
        if base is a child_s (shadowed) { };
        var maybe06: int = shadowed;
        if (base is a child_s (conflict)) and ((base is a other_s (conflict)) or TRUE) {
            var maybe07: string = conflict.number;
        };
        if base is a child_s (outer) {
            if base is a other_s (outer) { };
            var maybe08: string = outer.number;
        };
    };
};
