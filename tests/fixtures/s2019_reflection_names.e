<'
// T11.1 (clause 32.1, clause 32.2): the reflection catalogue names are
// recognised in type positions and as routine calls — a referencing
// file parses with no "unknown name" diagnostic. The calls report
// E0419 at run time; the recognised construction (`new
// instance_iterator`) is the one clause 32.2 surface built for real.
unit leaf_u {
    x: uint;
};
extend sys {
    o: rf_object;
    m: rf_manager;
    items: rf_check;
    lst: list of rf_struct_member;
    run() is also {
        var it: instance_iterator = new instance_iterator;
        var sub: unit_instance_iterator;
        var p: instance_path_item;
        get_user_template_types();
        eval_exp_string("1");
        insert_list_element(NULL);
    };
};
'>
