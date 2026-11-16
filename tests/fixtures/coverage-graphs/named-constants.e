struct bounds_s {
    static const top: int = start_value + 1;
    static const start_value: int = 0;
};
unit base_u {
    const low: int = 0;
    const high: int = low + bounds_s::top;
    const bucket_width: int = 1;
    const goal: int = 2;
    const importance: int = 3;
    const zero: int = 0;
    const pair_name: string = "pa" + "ir";
    const history_name: string = pair_name + "_history";
    !value: int = 0;
    cover cg using per_unit_instance, at_least = goal, weight = importance is {
        item n: int = value using ranges = { range([low..high], "", bucket_width, goal); }, weight = zero;
        item m: bool = FALSE using no_collect;
        cross n, m using name = pair_name;
        transition pair using name = history_name;
    };
    send(next: int) is { value = next; emit cg; };
};
unit derived_u like base_u {};
extend sys {
    a: derived_u is instance;
    setup() is also { set_config(cover, mode, normal); };
    run() is also { check that a.high == 1; check that a.history_name == "pair_history"; check that bounds_s::top == 1; a.send(0); a.send(1); a.send(0); a.send(1); };
};
