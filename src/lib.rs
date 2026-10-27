//! Experimental language tooling for the IEEE 1647 `e` verification language.
//!
//! **Not** a complete compiler or simulator. Implemented so far: a tokenizer
//! for a growing lexical subset ([`lexer`]) and a parser with a typed AST,
//! minimal well-formedness checks and spanned diagnostics
//! ([`parser`], [`ast`], [`check`]), member composition that folds
//! `like` chains and `extend` blocks into per-type member lists
//! ([`elab`]), and an interpreter that executes the non-temporal statement
//! subset, including `gen` as a seeded rejection sampler ([`interp`]).
//! See docs/ROADMAP.md for what is deliberately not implemented yet.

pub mod ast;
pub mod builtins;
pub mod check;
pub mod diagnostic;
pub mod elab;
pub mod interp;
pub mod lexer;
pub mod parser;
pub mod solve;
