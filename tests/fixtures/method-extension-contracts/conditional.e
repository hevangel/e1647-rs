type state_t: [active_state, idle_state];
struct conditional_s {
    !flag: state_t = active_state;
    f(x: int): int is { return x; };
    when active_state'flag {
        f(x: string) is also { };
    };
};
