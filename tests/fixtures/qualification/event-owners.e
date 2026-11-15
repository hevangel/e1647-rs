<'
unit node_u {
    event pulse;
    !hits: uint = 0;
    worker() @pulse is { hits = hits + 1; };
};
extend sys {
    a: node_u is instance;
    b: node_u is instance;
    run() is also {
        start a.worker(); start b.worker();
        wait [2]*cycle;
        emit a.pulse;
        wait [1]*cycle;
        out("E1647_RESULT first_a=", a.hits, "\n");
        out("E1647_RESULT first_b=", b.hits, "\n");
        emit b.pulse;
        wait [1]*cycle;
        out("E1647_RESULT final_a=", a.hits, "\n");
        out("E1647_RESULT final_b=", b.hits, "\n");
        out("E1647_DONE event-owners\n");
    };
};
'>
