struct box_s {
    size: uint;
};
struct owner_s {
    input: string;
    convert(input: uint): string is {
        var copy := input;
        return copy;
    };
    inspect(p: box_s, n: uint, names: list of string) is {
        var alias := p;
        alias.absent;
        n[0];
        var wrong: uint = names[0];
    };
    unrelated(p) is {
        p.unknown;
    };
};
