struct packet_s { };
struct other_s { };
interface contract_i {
    scalar(x: int): int;
    sized(x: uint(bits: 8));
    elements(x: list of int);
    arity(x: int);
    result(): bool;
    nominal(x: packet_s);
    typed(x: int);
    shape(x: list of int);
};
struct broken_s implementing contract_i {
    scalar(x: string): int is { return 0; };
    sized(x: uint(bits: 16)) is { };
    elements(x: list of string) is { };
    arity() is { };
    result(): int is { return 0; };
    nominal(x: other_s) is { };
    typed(x) is { };
    shape(x: packet_s) is { };
};
interface static_i { ping(); };
struct static_s implementing static_i { static ping() is { }; };
