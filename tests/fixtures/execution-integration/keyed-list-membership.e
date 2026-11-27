type codes_t: list (key: it) of uint;
struct entry_s {
    !code: uint = 4;
};
extend sys {
    !codes: codes_t;
    !entries: list (key: it.code) of entry_s;
    !calls: uint = 0;

    lookup(values: codes_t, code: uint): bool is {
        return values.key_exists(code);
    };
    get_codes(): codes_t is {
        calls = calls + 1;
        return codes;
    };
    run() is also {
        var alias: codes_t = codes;
        out(codes.key_exists(4), "\n");
        alias.add(4);
        alias.add(4);
        out(lookup(codes, 4), " ", codes.key_exists(5), "\n");
        out(get_codes().key_exists(4), " ", calls, "\n");
        var entry: entry_s = new entry_s;
        entries.add(entry);
        out(entries.key_exists(4), "\n");
        entry.code = 9;
        out(entries.key_exists(4), " ", entries.key_exists(9), "\n");
    };
};
