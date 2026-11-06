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
/// `var s: set` — the set type of 5.2.12).
pub const BUILTIN_TYPES: &[&str] = &["file", "event", "set"];

pub fn is_builtin_type(name: &str) -> bool {
    BUILTIN_TYPES.contains(&name)
}

pub fn is_builtin_routine(name: &str) -> bool {
    BUILTIN_ROUTINES.contains(&name)
}
