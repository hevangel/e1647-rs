//! A tokenizer for a growing subset of the `e` lexical grammar.
//!
//! Keyword recognition, multi-character operators and IEEE 1647-style
//! numerals (sized/unsized based literals, `0x`/`0b` prefixes, reals) were
//! cross-checked against the MIT-licensed grammars pinned under
//! `external/` (see docs/REFERENCES.md). A bare `'` followed by an
//! identifier is the subtype modifier quote (`read'kind`), tokenized as a
//! standalone punctuation token like the reference grammar's `SNG_QUOTE`.
//! Code-segment markers `<'` and `'>` (clause 4.1.2) count only when they
//! occupy the first column and the rest of that line is whitespace. A file
//! that contains such a begin marker is a comment until the first one, and
//! again between segments. A file with no begin marker is tokenized as code
//! throughout, so snippet tests need not wrap a segment. Decimal integers
//! may end in a `K`/`k` (×1024) or `M`/`m` (×1024×1024) multiplier
//! (clause 4.1.4.1). `0c"..."` is a one-character literal (clause 4.1.4.6).
//! Based literals may include the MVL digits `x z u l h w n`
//! (clause 4.1.4.3); evaluating those literals is a later stage.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    /// An exact-match e keyword such as `struct`, `keep` or `bits`.
    Keyword(String),
    Identifier(String),
    /// Any numeral: decimal (optional `K`/`M` multiplier), real, sized or
    /// unsized based literal (`8'b1010`, `'hFF`, MVL digits included),
    /// `0x`/`0b`/`0o` literal, or a `0c"..."` character literal. Raw text is
    /// preserved; interpretation belongs to later stages.
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
/// here (the reference grammar leaves them as identifiers), the clause 8
/// keyword `template` (Table 8 of the 2016 standard) is a keyword here —
/// the reference grammar predates it — multi-word
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
    "template",
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
/// e identity comparisons; `!~` is the negative string match; `=>` is
/// implication; `..` is a range; `:=` is the type-inferring `var`
/// initialization (clause 18).
const OPERATORS: &[&str] = &[
    "===", "!==", "==", "!=", "!~", "<=", ">=", "=>", "&&", "||", "<<", ">>", "..", "+=", "-=",
    ":=",
];

struct Cursor<'a> {
    source: &'a str,
    byte: usize,
    line: usize,
    column: usize,
    /// Whether this file has a column-0 `<'` marker line (clause 4.1.2).
    /// Without one, the whole source is code.
    segments: bool,
    /// Whether the cursor is inside a code segment. Prose outside segments
    /// is skipped when [`Self::segments`] is set.
    in_code: bool,
}

impl<'a> Cursor<'a> {
    fn new(source: &'a str) -> Self {
        let segments = has_begin_code_marker(source);
        Self {
            source,
            byte: 0,
            line: 1,
            column: 1,
            segments,
            in_code: !segments,
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
            if let Some(open) = self.line_is_code_marker() {
                if open && self.in_code {
                    return Err(LexError {
                        position: self.position(),
                        message: "code segment opened inside a code segment",
                    });
                }
                if open || self.in_code {
                    self.consume_rest_of_line();
                    self.in_code = open;
                    continue;
                }
            }
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
                // e's native line comment: `--` to end of line (IEEE 1647).
                (Some('-'), Some('-')) => {
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
                // Prose outside code segments is skipped. An unterminated
                // `<'` runs to end of source and the scan still terminates.
                _ if !self.in_code && self.peek().is_some() => {
                    self.bump();
                }
                _ => return Ok(()),
            }
        }
    }

    /// A begin-code (`<'`) or end-code (`'>`) marker: first column, and
    /// only whitespace after it on that line (clause 4.1.2). `Some(true)`
    /// is a begin marker.
    fn line_is_code_marker(&self) -> Option<bool> {
        if !self.segments || self.column != 1 {
            return None;
        }
        let rest = &self.source[self.byte..];
        let open = rest.starts_with("<'");
        if !open && !rest.starts_with("'>") {
            return None;
        }
        let mut tail = rest[2..].chars();
        for ch in tail.by_ref() {
            match ch {
                ' ' | '\t' => {}
                '\r' | '\n' => return Some(open),
                _ => return None,
            }
        }
        Some(open)
    }

    fn consume_rest_of_line(&mut self) {
        while let Some(ch) = self.bump() {
            if ch == '\n' {
                break;
            }
        }
    }

    fn read_word(&mut self) -> String {
        let start_byte = self.byte;
        // `$` extends identifiers: `port$` is the simple_port value access.
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
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
    /// spellings: `x`/`X` per the reference grammar, `h`/`H` per clause
    /// 4.1.4.2 and common Specman usage, `f`/`F` per the reference grammar.
    /// Non-decimal bases also accept MVL digits `x z u l h w n`
    /// (clause 4.1.4.3). Decimal bases do not. `0x`/`0b`/`0o` prefixes are
    /// unsized numbers (clause 4.1.4.1) and do not take MVL digits.
    fn read_based_digits(
        &mut self,
        base: char,
        start: Position,
        mvl: bool,
    ) -> Result<(), LexError> {
        if !self.peek().is_some_and(|c| based_digit_ok(base, c, mvl)) {
            return Err(LexError {
                position: start,
                message: "invalid based literal",
            });
        }
        while self.peek().is_some_and(|c| based_digit_ok(base, c, mvl)) {
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

    fn finish_number(&self, start: Position) -> TokenKind {
        TokenKind::Number(self.source[start.byte..self.byte].to_owned())
    }

    /// `0c"..."` / `0C"..."`: one ASCII character, or one string-style
    /// escape, between the quotes (clause 4.1.4.6).
    fn read_character_literal(&mut self, start: Position) -> Result<TokenKind, LexError> {
        self.bump();
        self.bump();
        self.bump();
        let Some(ch) = self.bump() else {
            return Err(LexError {
                position: start,
                message: "unterminated character literal",
            });
        };
        if ch == '\n' || ch == '\r' {
            return Err(LexError {
                position: start,
                message: "unterminated character literal",
            });
        }
        if ch == '"' {
            return Err(LexError {
                position: start,
                message: "invalid character literal",
            });
        }
        if ch == '\\' {
            let Some(escaped) = self.bump() else {
                return Err(LexError {
                    position: start,
                    message: "unterminated character literal",
                });
            };
            if !matches!(escaped, 'n' | 't' | 'r' | '\\' | '"' | '0') {
                return Err(LexError {
                    position: start,
                    message: "invalid character literal",
                });
            }
        }
        if self.peek() != Some('"') {
            return Err(LexError {
                position: start,
                message: "invalid character literal",
            });
        }
        self.bump();
        Ok(self.finish_number(start))
    }

    /// A single `K`/`k` or `M`/`m` glued to a decimal integer, and not the
    /// start of a longer name (`32KEEP` stays a number and an identifier).
    fn take_decimal_multiplier(&mut self) {
        if !matches!(self.peek(), Some('K' | 'k' | 'M' | 'm')) {
            return;
        }
        if self
            .peek_second()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
        {
            return;
        }
        self.bump();
    }

    /// Numerals: decimal, real with optional exponent, sized/unsized based
    /// literal (`8'b1010`, `'hFF`, MVL digits), `0x`/`0b`/`0o` prefixes,
    /// `K`/`M` multipliers, and `0c"..."` character literals.
    fn read_number(&mut self) -> Result<TokenKind, LexError> {
        let start = self.position();
        if self.peek() == Some('0')
            && matches!(self.peek_second(), Some('c') | Some('C'))
            && self.source[self.byte..].chars().nth(2) == Some('"')
        {
            return self.read_character_literal(start);
        }
        // 0x / 0b / 0o prefixed literals. These are unsized numbers, so
        // they do not take MVL digits or K/M multipliers.
        if self.peek() == Some('0')
            && matches!(
                self.peek_second(),
                Some('x') | Some('X') | Some('b') | Some('B') | Some('o') | Some('O')
            )
        {
            self.bump();
            let base = self.bump().expect("peeked prefix letter");
            self.read_based_digits(base, start, false)?;
            return Ok(self.finish_number(start));
        }
        self.read_decimal_digits();
        // Sized based literal: digits ' base digits. No K/M multiplier.
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
            self.read_based_digits(base, start, true)?;
            return Ok(self.finish_number(start));
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
        // K/M apply only to a decimal integer, not to a real or an exponent.
        let spelling = &self.source[start.byte..self.byte];
        if !spelling
            .bytes()
            .any(|byte| byte == b'.' || byte == b'e' || byte == b'E')
        {
            self.take_decimal_multiplier();
        }
        Ok(self.finish_number(start))
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
            // Either a based literal (`'hFF`, including MVL digits), the
            // subtype modifier quote (`read'kind`), or the opening or
            // closing quote of a quoted hierarchical path
            // (`'memory_tb.address'`). Non-literal quotes become standalone
            // punctuation tokens (like the reference grammar's `SNG_QUOTE`);
            // the parser decides what they delimit.
            match self.peek_second() {
                Some(base) if Self::based_base_letter(base) => {
                    self.bump();
                    self.bump();
                    self.read_based_digits(base, start, true)?;
                    self.finish_number(start)
                }
                _ => {
                    self.bump();
                    TokenKind::Punctuation("'".to_owned())
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

/// Whether `c` is legal in a based literal of `base`. `mvl` allows the
/// clause 4.1.4.3 digits on binary, octal, and hexadecimal values.
fn based_digit_ok(base: char, c: char, mvl: bool) -> bool {
    let mvl_letter = mvl
        && matches!(
            c,
            'x' | 'X' | 'z' | 'Z' | 'u' | 'U' | 'l' | 'L' | 'h' | 'H' | 'w' | 'W' | 'n' | 'N'
        );
    match base {
        'b' | 'B' => matches!(c, '0' | '1' | '_') || mvl_letter,
        'o' | 'O' => (c.is_ascii_digit() && c <= '7') || c == '_' || mvl_letter,
        'd' | 'D' => c.is_ascii_digit() || c == '_',
        'x' | 'X' | 'h' | 'H' | 'f' | 'F' => c.is_ascii_hexdigit() || c == '_' || mvl_letter,
        _ => false,
    }
}

/// Clause 4.1.2: a begin-code marker is `<'` at the first column with only
/// whitespace after it on that line.
fn has_begin_code_marker(source: &str) -> bool {
    let mut at_line_start = true;
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if at_line_start && bytes[index..].starts_with(b"<'") {
            let mut cursor = index + 2;
            while cursor < bytes.len() && (bytes[cursor] == b' ' || bytes[cursor] == b'\t') {
                cursor += 1;
            }
            if cursor == bytes.len() || bytes[cursor] == b'\n' || bytes[cursor] == b'\r' {
                return true;
            }
        }
        match bytes[index] {
            b'\n' => at_line_start = true,
            b'\r' => {}
            _ => at_line_start = false,
        }
        index += 1;
    }
    false
}

/// Tokenize source. The lexer reports the first lexical error; later stages
/// collect multiple diagnostics.
pub fn tokenize(source: &str) -> Result<Vec<Token>, LexError> {
    let mut cursor = Cursor::new(source);
    let mut result = Vec::new();
    while let Some(token) = cursor.next_token()? {
        result.push(token);
    }
    Ok(result)
}
