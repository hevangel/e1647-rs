<'
extend sys {
    run() is also {
        table { #a | #b ;
                1 | 2 ;
                3 } with { out(<#1>); };
    };
};
'>
