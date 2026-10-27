//! A tokenizer for a growing subset of the `e` lexical grammar.
//!
//! Keyword recognition, multi-character operators and IEEE 1647-style
//! numerals (sized/unsized based literals, `0x`/`0b` prefixes, reals) were
//! cross-checked against the MIT-licensed grammars pinned under
//! `external/` (see docs/REFERENCES.md). A bare `'` followed by an
//! identifier is the subtype modifier quote (`read'kind`), tokenized as a
//! standalone punctuation token like the reference grammar's `SNG_QUOTE`.
//! The e code-block delimiters `<'` (open) and `'>` (close) are recognized
//! and skipped as trivia; their contents are ordinary code in this subset.
//! Character literals are deliberately not supported yet and produce
//! explicit errors.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    /// An exact-match e keyword such as `struct`, `keep` or `bits`.
    Keyword(String),
    Identifier(String),
    /// Any numeral: decimal, real, sized based literal (`8'b1010`),
    /// unsized based literal (`'hFF`) or `0x`/`0b` literal. Raw text is
    /// preserved; interpretation belongs to the parser.
    Number(String),
    StringLiteral(String),
    Punctuation(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub byte: usize,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub start: Position,
    pub end: Position,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub position: Position,
    pub message: &'static str,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: {}",
            self.position.line, self.position.column, self.message
        )
    }
}

impl std::error::Error for LexError {}

/// Exact-match keywords of the e language. Uppercase `TRUE`/`FALSE`/`NULL`
/// are keywords, not identifiers. The set is cross-checked item-by-item
/// against the MIT-licensed reference grammar in
/// `external/specman-e-parser/specman.flex` by
/// `tests/lexer.rs::keywords_match_reference_grammar_set`. Deviations:
/// the IEEE 1647 predefined type spellings `real` and `string` are keywords
/// here (the reference grammar leaves them as identifiers), multi-word
/// reference keywords (`is a`, `state machine`, `C export`, `C routine`)
/// are not single-token keywords, and their operator spellings are handled
/// by later stages.
pub const KEYWORDS: &[&str] = &[
    "all",
    "also",
    "and",
    "as",
    "as_a",
    "as_computed",
    "assert",
    "assume",
    "at_least",
    "before",
    "bin",
    "bind",
    "bit",
    "bits",
    "bool",
    "break",
    "byte",
    "bytes",
    "c_export",
    "c_routine",
    "case",
    "change",
    "check",
    "compute",
    "consume",
    "continue",
    "cover",
    "cross",
    "cycle",
    "dec",
    "default",
    "define",
    "delay",
    "detach",
    "do",
    "down",
    "e_path",
    "each",
    "else",
    "emit",
    "empty",
    "enclosing",
    "event",
    "eventually",
    "exec",
    "expect",
    "extend",
    "fail",
    "fall",
    "FALSE",
    "file",
    "first",
    "for",
    "force",
    "from",
    "gen",
    "global",
    "hex",
    "if",
    "ignore",
    "illegal",
    "import",
    "in",
    "index",
    "instance",
    "int",
    "is",
    "item",
    "it",
    "keep",
    "keeping",
    "key",
    "like",
    "line",
    "list",
    "me",
    "method",
    "matching",
    "nand",
    "new",
    "nibble",
    "no_collect",
    "no_trace",
    "nor",
    "not",
    "now",
    "num_of_buckets",
    "nxor",
    "of",
    "on",
    "only",
    "or",
    "package",
    "per_instance",
    "per_unit_instance",
    "port",
    "prev",
    "print",
    "private",
    "protected",
    "radix",
    "range",
    "real",
    "release",
    "repeat",
    "reverse",
    "return",
    "rise",
    "routine",
    "select",
    "sequence",
    "simple",
    "soft",
    "start",
    "state_machine",
    "step",
    "string",
    "struct",
    "sync",
    "that",
    "text",
    "then",
    "time",
    "to",
    "transition",
    "TRUE",
    "try",
    "type",
    "undefined",
    "unit",
    "until",
    "using",
    "uint",
    "var",
    "wait",
    "weight",
    "when",
    "while",
    "with",
    "within",
    "xor",
    "true",
    "false",
    "NULL",
];

/// Whether `word` is an exact-match e keyword. A keyword prefix
/// (`structs`) is still a plain identifier. Lookup is order-independent on
/// purpose: keep the list grouped for cross-checks instead of maintaining a
/// sorted order.
pub fn is_keyword(word: &str) -> bool {
    KEYWORDS.contains(&word)
}

/// Multi-character operators, longest match first. `===`/`!==` are the
/// e identity comparisons; `=>` is implication; `..` is a range.
const OPERATORS: &[&str] = &[
    "===", "!==", "==", "!=", "<=", ">=", "=>", "&&", "||", "<<", ">>", "..",
];

struct Cursor<'a> {
    source: &'a str,
    byte: usize,
    line: usize,
    column: usize,
}

impl<'a> Cursor<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            byte: 0,
            line: 1,
            column: 1,
        }
    }

    fn position(&self) -> Position {
        Position {
            byte: self.byte,
            line: self.line,
            column: self.column,
        }
    }

    fn peek(&self) -> Option<char> {
        self.source[self.byte..].chars().next()
    }

    fn peek_second(&self) -> Option<char> {
        let mut chars = self.source[self.byte..].chars();
        chars.next()?;
        chars.next()
    }

    fn bump(&mut self) -> Option<char> {
        let value = self.peek()?;
        self.byte += value.len_utf8();
        if value == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(value)
    }

    fn skip_trivia(&mut self) -> Result<(), LexError> {
        loop {
            match (self.peek(), self.peek_second()) {
                (Some(ch), _) if ch.is_whitespace() => {
                    self.bump();
                }
                (Some('/'), Some('/')) => {
                    self.bump();
                    self.bump();
                    while let Some(ch) = self.peek() {
                        if ch == '\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                (Some('/'), Some('*')) => {
                    let start = self.position();
                    self.bump();
                    self.bump();
                    let mut closed = false;
                    while let Some(ch) = self.bump() {
                        if ch == '*' && self.peek() == Some('/') {
                            self.bump();
                            closed = true;
                            break;
                        }
                    }
                    if !closed {
                        return Err(LexError {
                            position: start,
                            message: "unterminated block comment",
                        });
                    }
                }
                // e code-block delimiters: `<'` opens and `'>` closes;
                // recognized and skipped, their contents are ordinary code
                // in this subset.
                (Some('<'), Some('\'')) | (Some('\''), Some('>')) => {
                    self.bump();
                    self.bump();
                }
                _ => return Ok(()),
            }
        }
    }

    fn read_word(&mut self) -> String {
        let start_byte = self.byte;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            self.bump();
        }
        self.source[start_byte..self.byte].to_owned()
    }

    /// Decimal digits and underscores; requires at least one leading digit.
    fn read_decimal_digits(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '_') {
            self.bump();
        }
    }

    /// Digits legal for a based literal, validated per base. Hexadecimal
    /// spellings: `x`/`X` per the reference grammar, `h`/`H` per common
    /// Specman usage (confirmed by the Specman 2025.03 reference run,
    /// docs/VERIFICATION.md 2026-10-27), `f`/`F` per the reference grammar.
    fn read_based_digits(&mut self, base: char, start: Position) -> Result<(), LexError> {
        let accepted = match base {
            'b' | 'B' => |c: char| matches!(c, '0' | '1' | '_'),
            'o' | 'O' => |c: char| c.is_ascii_digit() && c <= '7' || c == '_',
            'd' | 'D' => |c: char| c.is_ascii_digit() || c == '_',
            'x' | 'X' | 'h' | 'H' | 'f' | 'F' => |c: char| c.is_ascii_hexdigit() || c == '_',
            _ => unreachable!("caller validates the base letter"),
        };
        if !self.peek().is_some_and(accepted) {
            return Err(LexError {
                position: start,
                message: "invalid based literal",
            });
        }
        while self.peek().is_some_and(accepted) {
            self.bump();
        }
        Ok(())
    }

    /// The base letter after `'` in a based literal. Hexadecimal is spelled
    /// `x`/`X`/`f`/`F` in the reference grammar and additionally `h`/`H` in
    /// common Specman usage.
    fn based_base_letter(c: char) -> bool {
        matches!(
            c,
            'b' | 'B' | 'o' | 'O' | 'd' | 'D' | 'x' | 'X' | 'h' | 'H' | 'f' | 'F'
        )
    }

    /// Numerals: decimal, real with optional exponent, sized/unsized based
    /// literal (`8'b1010`, `'hFF`), and `0x`/`0b`/`0o` prefixed literals.
    fn read_number(&mut self) -> Result<TokenKind, LexError> {
        let start = self.position();
        // 0x / 0b / 0o prefixed literals.
        if self.peek() == Some('0')
            && matches!(
                self.peek_second(),
                Some('x') | Some('X') | Some('b') | Some('B') | Some('o') | Some('O')
            )
        {
            self.bump();
            let base = self.bump().expect("peeked prefix letter");
            self.read_based_digits(base, start)?;
            return Ok(TokenKind::Number(
                self.source[start.byte..self.byte].to_owned(),
            ));
        }
        self.read_decimal_digits();
        // Sized based literal: digits ' base digits.
        if self.peek() == Some('\'') {
            self.bump();
            let Some(base) = self.peek() else {
                return Err(LexError {
                    position: start,
                    message: "invalid based literal",
                });
            };
            if !Self::based_base_letter(base) {
                return Err(LexError {
                    position: start,
                    message: "invalid based literal",
                });
            }
            self.bump();
            self.read_based_digits(base, start)?;
            return Ok(TokenKind::Number(
                self.source[start.byte..self.byte].to_owned(),
            ));
        }
        // Real fraction: only when `.` is followed by a digit, so `1..5`
        // lexes as a number, a range and a number.
        if self.peek() == Some('.') && self.peek_second().is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
            self.read_decimal_digits();
        }
        // Exponent, only when a digit (or sign plus digit) follows.
        if matches!(self.peek(), Some('e') | Some('E')) {
            let mut lookahead = self.source[self.byte + 1..].chars();
            let exponent_starts = match lookahead.next() {
                Some(c) if c.is_ascii_digit() => true,
                Some('+' | '-') => lookahead.next().is_some_and(|c| c.is_ascii_digit()),
                _ => false,
            };
            if exponent_starts {
                self.bump();
                if matches!(self.peek(), Some('+') | Some('-')) {
                    self.bump();
                }
                self.read_decimal_digits();
            }
        }
        Ok(TokenKind::Number(
            self.source[start.byte..self.byte].to_owned(),
        ))
    }

    fn read_operator(&mut self) -> TokenKind {
        for candidate in OPERATORS {
            if self.source[self.byte..].starts_with(candidate) {
                for _ in 0..candidate.chars().count() {
                    self.bump();
                }
                return TokenKind::Punctuation((*candidate).to_owned());
            }
        }
        let symbol = self.bump().expect("caller ensured a character");
        TokenKind::Punctuation(symbol.to_string())
    }

    fn next_token(&mut self) -> Result<Option<Token>, LexError> {
        self.skip_trivia()?;
        let start = self.position();
        let Some(first) = self.peek() else {
            return Ok(None);
        };
        let kind = if first.is_ascii_alphabetic() || first == '_' {
            let word = self.read_word();
            if is_keyword(&word) {
                TokenKind::Keyword(word)
            } else {
                TokenKind::Identifier(word)
            }
        } else if first.is_ascii_digit() {
            self.read_number()?
        } else if first == '"' {
            self.bump();
            let mut value = String::new();
            let mut closed = false;
            while let Some(ch) = self.bump() {
                if ch == '"' {
                    closed = true;
                    break;
                }
                if ch == '\\' {
                    let Some(escaped) = self.bump() else {
                        break;
                    };
                    value.push('\\');
                    value.push(escaped);
                } else {
                    value.push(ch);
                }
            }
            if !closed {
                return Err(LexError {
                    position: start,
                    message: "unterminated string literal",
                });
            }
            TokenKind::StringLiteral(value)
        } else if first == '\'' {
            // Either a based literal (`'hFF`), the subtype modifier quote
            // (`read'kind`; a standalone token, like the reference grammar's
            // `SNG_QUOTE`), or — until a later milestone — an unsupported
            // character literal.
            match self.peek_second() {
                Some(base) if Self::based_base_letter(base) => {
                    self.bump();
                    self.bump();
                    self.read_based_digits(base, start)?;
                    TokenKind::Number(self.source[start.byte..self.byte].to_owned())
                }
                Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                    self.bump();
                    TokenKind::Punctuation("'".to_owned())
                }
                _ => {
                    return Err(LexError {
                        position: start,
                        message: "character literals are not supported yet",
                    });
                }
            }
        } else {
            self.read_operator()
        };
        Ok(Some(Token {
            kind,
            start,
            end: self.position(),
        }))
    }
}

/// Tokenize source. The lexer reports the first lexical error; later stages
/// collect multiple diagnostics. Future versions will add character
/// literals, `<' .. '>` code literals and remaining operator spellings.
pub fn tokenize(source: &str) -> Result<Vec<Token>, LexError> {
    let mut cursor = Cursor::new(source);
    let mut result = Vec::new();
    while let Some(token) = cursor.next_token()? {
        result.push(token);
    }
    Ok(result)
}
