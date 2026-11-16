type state_t: [active_state, idle_state];
struct conditional_s {
    !mode: state_t = active_state;
    f(x: int): int is { return x; };
    when active_state'mode {
        f(x: string) is also { };
    };
};
