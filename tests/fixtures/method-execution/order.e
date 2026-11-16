struct helper_s {
    trace() is { out("core "); };
    trace() is also { out("a1 "); };
    trace() is first { out("f1 "); };
    trace() is first { out("f2 "); };
    trace() is also { out("a2 "); };
    replaced() is { out("discard-core "); };
    replaced() is first { out("discard-first "); };
    replaced() is also { out("discard-also "); };
    replaced() is only { out("only "); };
    replaced() is also { out("after-only "); };
    replaced() is first { out("before-only "); };
};
struct base_s {
    trace() is { out("discard-parent "); };
    trace() is first { out("parent-first "); };
    trace() is also { out("parent-also "); };
};
struct child_s like base_s {
    trace() is only { out("child-only "); };
    trace() is first { out("child-first "); };
    trace() is also { out("child-also "); };
};
extend sys {
    run() is also {
        var helper: helper_s = new helper_s;
        helper.trace(); out("end\n");
        helper.replaced(); out("end\n");
        var child: child_s = new child_s;
        child.trace(); out("end\n");
    };
};
