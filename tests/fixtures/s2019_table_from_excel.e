<'
extend sys {
    run() is also {
        table from excel_to_table("tests/data/regs", "sheet", "regs") with { out("x"); };
    };
};
'>
