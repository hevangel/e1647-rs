struct box_s {
    !n: uint = 10;
    read(a: uint, b: uint): uint is { sync; return n + a + b; };
};
extend sys {
    !box: box_s;
    !receivers: uint = 0;
    !args: uint = 0;
    receiver(): box_s is { receivers = receivers + 1; sync; return box; };
    dynamic() is { receivers = receivers + 1; sync; return box; };
    argument(): uint is { args = args + 1; sync; return args; };
    later(n: uint): uint is { sync; return n; };
    nested(n: uint): uint is { return later(n) + later(n + 1); };
    run() is also {
        box = new box_s;
        var answer: uint = receiver().read(argument(), nested(argument()));
        out(answer, " receivers ", receivers, " args ", args, "\n");
        dynamic().read(argument(), later(0));
        out("after ", receivers, " args ", args, "\n");
    };
};
