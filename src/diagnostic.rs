//! Spanned diagnostics shared by the parser front-end stages.
//!
//! The lexer keeps its single-error [`crate::lexer::LexError`] contract;
//! everything above the lexer reports through [`Diagnostic`] so one run can
//! surface every problem it found.

use crate::lexer::Position;
use std::fmt;

/// A spanned parse or well-formedness problem.
///
/// Codes are stable textual identifiers documented in `src/parser.rs`:
/// `E01xx` for syntax errors, `E02xx` for well-formedness errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub start: Position,
    pub end: Position,
}

impl Diagnostic {
    pub fn new(
        code: &'static str,
        message: impl Into<String>,
        start: Position,
        end: Position,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            start,
            end,
        }
    }

    /// Render as `line:column: error[code]: message` (no file name; callers
    /// that know the source path prepend it).
    pub fn render(&self) -> String {
        format!(
            "{}:{}: error[{}]: {}",
            self.start.line, self.start.column, self.code, self.message
        )
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render())
    }
}
