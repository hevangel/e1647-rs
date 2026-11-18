struct item_s { !value: int; };
sequence seq_s using item = item_s, created_driver = driver_u;
extend driver_u { !count: int = 0; };
extend seq_s {
    body() is also { driver.count = 1; };
    body() is first { driver.count = 2; return; };
    body() is also { driver.count = driver.count + 10; };
};
extend sys {
    !driver: driver_u;
    run() is also {
        check that driver.count == 12;
        out("sequence wrappers passed\n");
    };
};
