<'
extend sys {
    run() is also {
        var repeated: list of uint = { 1; 1; };
        var single: list of uint = { 1; 2; };
        var enough: list of uint = { 2; 1; 1; };
        var insufficient: uint = 0;
        var contained: uint = 0;
        if repeated in single { insufficient = 1; };
        if repeated in enough { contained = 1; };
        out("E1647_RESULT insufficient=", insufficient, "\n");
        out("E1647_RESULT contained=", contained, "\n");
        out("E1647_RESULT source_size=", enough.size(), "\n");
        out("E1647_DONE list-containment\n");
    };
};
'>
