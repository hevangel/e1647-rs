<'
struct packet_s { !n: uint = 0; };
unit processor_u {
    consume(p: packet_s): uint is { return p.n; };
    value(): uint is { return evaluate { value = 9; }; };
};
extend sys {
    processor: processor_u is instance;
    run() is also {
        var seen: uint = processor.consume(new with { n = 7; });
        out("E1647_RESULT constructor=", seen, "\n");
        out("E1647_RESULT returned=", processor.value(), "\n");
        out("E1647_DONE contextual-values\n");
    };
};
'>
