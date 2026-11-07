<'
struct regs {
    table { #name | #width ;
            ctrl  | 8      ;
            stat  | 4      ;
            data  | 16     } with {
        <#name>: uint(bits: <#width>);
        keep <#name> < <#width>;
    };
};
extend sys {
    run() is also {
        var r: regs; gen r;
        table [1..3] with { out("row <#0> value <#1>"); };
        table count { a | 1; b | 2 } with { out("rows: <#1>"); };
    };
};
'>
