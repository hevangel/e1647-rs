struct worker_s {
    act(n: uint) is {
        out("start ", n, "\n");
        sync;
        out("end ", n, "\n");
    };
    down() is { sync; out("down\n"); };
};
extend sys {
    !w: worker_s;
    !receivers: uint = 0;
    !arguments: uint = 0;
    receiver(): worker_s is { receivers = receivers + 1; return w; };
    argument(): uint is { arguments = arguments + 1; return arguments; };
    peer() @sys.any is { out("peer\n"); };
    run() is also {
        w = new worker_s;
        start peer();
        receiver().act(argument());
        receiver().down();
        out(receivers, " ", arguments, "\n");
    };
};
