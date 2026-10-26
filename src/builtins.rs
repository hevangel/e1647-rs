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
/// concatenated, without a trailing newline.
pub const BUILTIN_ROUTINES: &[&str] = &["out"];

pub fn is_builtin_unit(name: &str) -> bool {
    BUILTIN_UNITS.contains(&name)
}

pub fn is_builtin_routine(name: &str) -> bool {
    BUILTIN_ROUTINES.contains(&name)
}
