<'
extend sys {
    run() is also {
        table from csv_to_table("tests/data/regs", "regs")
            using filter = table_column("name") != "dummy"
            with { out("<#name>:<#width>"); };
    };
};
'>
