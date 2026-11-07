//! Predeclared names of the e environment.
//!
//! The e language defines a small set of built-in types and routines that
//! exist without a declaration in the source file. Only the ones the
//! executed subset needs are listed here, and only by name — no built-in
//! behavior is attached to them beyond what the checker, elaborator and
//! interpreter explicitly implement.

/// Predeclared unit types. `sys` is the root unit whose `run()` methods
/// execute when a file is run.
pub const BUILTIN_UNITS: &[&str] = &["sys"];

/// Built-in routines callable from any method. `out` writes its arguments
/// concatenated, without a trailing newline. The `hdl_*` routines drive an
/// external SystemVerilog simulator through the VPI bridge
/// ([`crate::hdl`], the compose `mixed` service): `hdl_load` elaborates a
/// design, `hdl_set`/`hdl_get` assign and read signals by hierarchical
/// path, `hdl_run` advances time, `hdl_finish` ends the simulation.
pub const BUILTIN_ROUTINES: &[&str] = &[
    "out",
    "outf",
    "hdl_load",
    "hdl_set",
    "hdl_get",
    "hdl_run",
    "hdl_finish",
    // `dut_error`/`error` report a check failure and count it; the run
    // continues, as in Specman.
    "dut_error",
    "error",
    // `message(severity, ...)` prints when verbosity allows; the plain
    // subset always prints the message text.
    "message",
    // `append(a, b, ...)` concatenates strings (list append is a later
    // milestone).
    "append",
    // `stop_run()` ends the test: it unwinds all TCMs and stops the run
    // loop.
    "stop_run",
    // `pack(NULL, item)` concatenates the item's physical fields
    // MSB-first into one integer.
    "pack",
    // eRM objection protocol: raise/drop coordinate end-of-test; the
    // inline execution subset treats them as no-ops.
    "raise_objection",
    "drop_objection",
    // `set_check(name, effect [, root_unit])` (16.3.6): sets the check
    // effect of a named check action. The check-effect model is a later
    // milestone; the call is accepted (arity-checked) and ignored.
    "set_check",
    // `do_bind()` (9.13.3.2): performs the declarative port bindings.
    // Legal only in the connect_ports() generation sub-phase (9.13.3);
    // elsewhere the interpreter reports E0420.
    "do_bind",
    // `swap(<exp>)` (19.1): a predefined pack tool; recognised here, the
    // byte-order swap itself is not implemented (E0419 at run time).
    "swap",
    // `deep_copy(<exp>)` (29.2.2): the general deep copy — scalars,
    // strings and sets are values, a list copy mints a fresh object of
    // deep-copied items, a struct copy is a fresh instance with deep
    // copied fields (its `copy()` blocks run).
    "deep_copy",
    // The clause 29.13 range-generation routines over a `~` field
    // (clause 6.9.7): `rgf_was_generated(f)`, `rgf_get_range(f)` (the
    // generated reduced set of 10.3.13) and `rgf_was_reduced(f)`.
    "rgf_was_generated",
    "rgf_get_range",
    "rgf_was_reduced",
    // The clause 35.4 annotation API's element-taking calls and the
    // definition-element surface (T10.3): recognised, arity-checked,
    // E0419 at run time — only `@T.get_all_annotations()` is
    // implemented, dispatched in the interpreter on the annotation-type
    // receiver.
    "get_attached_annotation",
    "get_attached_annotations_list",
    "get_definition_element",
    "is_applicable",
    // The clause 32.1.2.17/32.1.4 `rf_manager` query surface (T11.1):
    // without a reflection model there is no `rf_manager` value to
    // receive them, so they are recognised as bare routine calls and
    // report E0419 at run time (the UNIMPLEMENTED_2019 table carries
    // the clause references). The catalogue:
    // docs/REFLECTION_2019.md.
    "get_annotation_type_by_name",
    "get_template_type_by_name",
    "get_template_by_name",
    "get_template_interface_by_name",
    "get_user_template_types",
    "get_user_templates",
    "eval_exp_string",
    "insert_list_element",
    "insert_list_element_unsafe",
    "delete_list_element",
    "delete_list_element_unsafe",
];

/// Predefined base units whose extends compose into every unit of the
/// kind: `any_unit` (clause 7.4 — the message hooks of 24.9.2.14 are
/// extendable on it). `any_struct` and the sequence bases stay
/// like-only targets — their extends have no composition path here and
/// are rejected rather than silently dropped.
pub const EXTENDABLE_BASES: &[&str] = &["any_unit"];

pub fn is_builtin_unit(name: &str) -> bool {
    BUILTIN_UNITS.contains(&name)
}

/// Predeclared base types a `struct … like …` may descend from
/// (`any_sequence_item` is the base of sequence items).
pub const BUILTIN_BASES: &[&str] = &["any_sequence_item", "any_sequence", "any_struct"];

pub fn is_builtin_base(name: &str) -> bool {
    BUILTIN_BASES.contains(&name)
}

/// Predeclared value types usable in type positions without a
/// declaration in the source file (`var f: file`, `ev: event;`,
/// `var s: set` — the set type of 5.2.12; `t: table` and
/// `r: table_row` — the 32.3 table API; `table_manager` is the
/// singleton receiver's type; `it: instance_iterator` — the 32.2
/// instance iterator, the one reflection surface implemented for real
/// (T11.2)).
pub const BUILTIN_TYPES: &[&str] = &[
    "file",
    "event",
    "set",
    "table",
    "table_row",
    "table_manager",
    "instance_iterator",
];

pub fn is_builtin_type(name: &str) -> bool {
    BUILTIN_TYPES.contains(&name) || is_recognized_reflection_type(name)
}

/// The clause 32.1/32.2 reflection type names this stage recognises but
/// does not model (T11.1, T11.2): `(name, clause)`. They resolve in
/// type positions (a `var o: rf_object` declaration is accepted), while
/// constructing one — `new rf_object`, `new instance_path_item`, `new
/// unit_instance_iterator` — reports `E0419` "'T' is recognised (clause
/// X) but not implemented in this stage" at run time, and the type's
/// default value is NULL. The catalogue with the per-clause method
/// names: `docs/REFLECTION_2019.md`.
pub const RECOGNIZED_REFLECTION_TYPES: &[(&str, &str)] = &[
    // 32.1.2.3: rf_object is the base of rf_struct and rf_interface.
    ("rf_object", "32.1.2.3"),
    ("rf_struct", "32.1.2.3"),
    ("rf_interface", "32.1.2.3"),
    // 32.1.2.5.2-32.1.2.5.13: struct-member descriptors.
    ("rf_struct_member", "32.1.2.5.2"),
    ("rf_range_generated_field", "32.1.2.5.4"),
    ("rf_on_method", "32.1.2.5.8-32.1.2.5.10"),
    ("rf_on_event_method", "32.1.2.5.8-32.1.2.5.10"),
    ("rf_on_event_path_method", "32.1.2.5.8-32.1.2.5.10"),
    ("rf_event", "32.1.2.5.11"),
    ("rf_check", "32.1.2.5.13"),
    // 32.1.2.6-32.1.2.7: interfaces.
    ("rf_interface_member", "32.1.2.6-32.1.2.7"),
    ("rf_interface_method", "32.1.2.6-32.1.2.7"),
    // 32.1.2.8.5: rf_like_struct.
    ("rf_like_struct", "32.1.2.8.5"),
    // 32.1.2.10: scalars and numerics.
    ("rf_scalar", "32.1.2.10"),
    ("rf_numeric", "32.1.2.10"),
    ("rf_custom_numeric", "32.1.2.10"),
    // 32.1.2.13: annotations.
    ("rf_annotation", "32.1.2.13"),
    ("rf_non_repeatable_annotation", "32.1.2.13"),
    ("rf_repeatable_annotation", "32.1.2.13"),
    // 32.1.2.14: the template family.
    ("rf_template_type", "32.1.2.14"),
    ("rf_template", "32.1.2.14"),
    ("rf_template_instance", "32.1.2.14"),
    ("rf_template_interface", "32.1.2.14"),
    ("rf_template_interface_instance", "32.1.2.14"),
    ("rf_template_numeric", "32.1.2.14"),
    ("rf_template_numeric_instance", "32.1.2.14"),
    ("rf_template_parameter", "32.1.2.14"),
    ("rf_type_template_parameter", "32.1.2.14"),
    ("rf_value_template_parameter", "32.1.2.14"),
    ("rf_template_instance_parameter", "32.1.2.14"),
    ("rf_type_template_instance_parameter", "32.1.2.14"),
    ("rf_value_template_instance_parameter", "32.1.2.14"),
    // 32.1.2.17: the rf_manager queries.
    ("rf_manager", "32.1.2.17"),
    // 32.1.3: definition elements, layers, modules.
    ("rf_definition_element", "32.1.3"),
    ("rf_struct_layer", "32.1.3"),
    ("rf_template_layer", "32.1.3"),
    ("rf_template_instance_layer", "32.1.3"),
    ("rf_module", "32.1.3"),
    // 32.1.4: value and manager utilities.
    ("rf_type", "32.1.4"),
    // 32.2: the instance-iterator surface. The base instance_iterator
    // is implemented (BUILTIN_TYPES above); the path item and the
    // subtype iterators are construction stubs.
    ("instance_path_item", "32.2"),
    ("unit_instance_iterator", "32.2"),
    ("unit_and_port_instance_iterator", "32.2"),
    ("port_instance_iterator", "32.2"),
];

pub fn is_recognized_reflection_type(name: &str) -> bool {
    RECOGNIZED_REFLECTION_TYPES
        .iter()
        .any(|(entry, _)| *entry == name)
}

/// The clause reference a recognized reflection type name carries —
/// the construction stub's E0419 message cites it.
pub fn reflection_type_clause(name: &str) -> Option<&'static str> {
    RECOGNIZED_REFLECTION_TYPES
        .iter()
        .find(|(entry, _)| *entry == name)
        .map(|(_, clause)| *clause)
}

pub fn is_builtin_routine(name: &str) -> bool {
    BUILTIN_ROUTINES.contains(&name)
}
