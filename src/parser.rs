//! Recursive-descent parser for the declared subset of the e language.
//!
//! Supported: `struct` / `unit` / `extend` / `type` declarations with
//! optional `like` inheritance, field declarations (single names and `a, b:`
//! name groups) with scalar (`uint(bits: 8)`-style widths included), `list
//! of`, and named types, enumeral type declarations, `keep` and `keep soft`
//! constraint expressions, `when` subtype members (`value'id` modifiers per
//! the reference grammar's `struct_type_modifier`), and method members with
//! parameter lists and statement bodies (local `var` declarations,
//! assignments, expression statements including calls and indexing,
//! `return`, braced `if`/`else`, `while` with `break`/`continue`, `print`
//! and `gen`).
//!
//! Everything else (events, temporal expressions, coverage, the
//! single-statement `if ... do` form, `for`/`repeat` loops) is a later
//! roadmap stage: the parser reports an explicit diagnostic and recovers at
//! declaration, member or statement boundaries. Under errors the AST is
//! partial — constructs that failed to parse are omitted, and the recorded
//! diagnostics are authoritative.
//!
//! ## Diagnostic codes
//!
//! - `E0001` lexical error (converted from [`crate::lexer::LexError`])
//! - `E0101` expected a top-level declaration (`struct`/`unit`/`extend`/`type`)
//! - `E0102` expected an identifier
//! - `E0103` expected `:`
//! - `E0104` expected a type
//! - `E0105` expected `;`
//! - `E0106` expected an opening delimiter (`{` or `[`)
//! - `E0107` expected a closing delimiter (`}` or `]`)
//! - `E0108` expected an expression
//! - `E0109` expected `(` in a method parameter list
//! - `E0110` expected `)`
//! - `E0111` expected `bits` in a width specification
//! - `E0112` expected `of` after `list`
//! - `E0113` unsupported member or statement kind
//! - `E0114` reserved (formerly "unsupported construct"; every construct it
//!   covered is now either supported or has its own code)
//! - `E0115` expected `is` in a method declaration
//! - `E0116` user-written `extend global` (clause 4.3.1.1)
//! - `E0117` unsupported or misplaced preprocessor directive (clause 21)
//! - `E0118` expected the match expression string of a macro definition
//!   (clause 15)
//! - `E0119` reserved (the temporal expression of an `expect`/`assume`
//!   member now reports `E0120` when missing)
//! - `E0120` unsupported, unexpected or missing temporal-expression
//!   construct (clause 12)
//! - `E0201` duplicate declaration name
//! - `E0202` duplicate field name
//! - `E0203` unknown type name
//! - `E0204` unknown identifier referenced in a `keep` constraint
//! - `E0205` width must be positive
//! - `E0206` duplicate name in an inner scope (parameter, local variable or
//!   enumeral)

use crate::ast::{
    BinaryOp, Declaration, DeclarationKind, Enumeral, Expr, Field, KeepConstraint, Member,
    MethodDecl, MethodModifier, OnDecl, Param, ScalarBase, Stmt, SubtypeModifier, TypeExpr,
    UnaryOp, WhenDecl,
};
use crate::diagnostic::Diagnostic;
use crate::lexer::{self, Position, Token, TokenKind};
use std::collections::BTreeSet;

/// Predefined e names that lex as keywords but appear as bare values or
/// call targets in real code (`set_config(print, radix, hex)`,
/// `pack(NULL, it)`, `it.x`, `wait until rise(sig)`). In expression and
/// statement position they are treated as plain identifiers.
const PREDEFINED_NAMES: &[&str] = &[
    "print",
    "radix",
    "hex",
    "dec",
    "bin",
    "items",
    "simulation",
    "NULL",
    "index",
    "it",
    "rise",
    "fall",
    "change",
    "TEST_DONE",
    // Method/field/var names of Table 8 keywords in real code (`me.x`,
    // `add(item: <type>)`, `key_exists(key)`, `result = new …` in the
    // etl corpus).
    "me",
    "item",
    "key",
    // Message-verbosity levels (`message(LOW, …)`).
    "LOW",
    "MEDIUM",
    "HIGH",
];

fn is_predefined_name(word: &str) -> bool {
    PREDEFINED_NAMES.contains(&word)
}

/// Binary operator rows from loosest to tightest binding (clause 4.5,
/// Table 12). Arithmetic, shifts, relational operators, and `&&` / `||`
/// follow C. `^` is looser than `|`, `===` / `!==` are looser than
/// `==` / `!=`. The word spellings (`and`, `or`, `xor`) share the level of
/// their symbol twins. All of these are left-associative. Three operators
/// of Table 12 are not rows here: `in` and `~` / `!~` sit between these
/// rows (see [`IN_LEVEL`], [`MATCH_LEVEL`]), and `is [not] a` has a type
/// operand instead of an expression (see [`ISA_LEVEL`]).
const BINARY_LEVELS: &[&[(&str, BinaryOp)]] = &[
    &[("=>", BinaryOp::Imply)],
    &[("||", BinaryOp::LogicalOr), ("or", BinaryOp::LogicalOr)],
    &[("&&", BinaryOp::LogicalAnd), ("and", BinaryOp::LogicalAnd)],
    &[("^", BinaryOp::BitXor), ("xor", BinaryOp::BitXor)],
    &[("|", BinaryOp::BitOr)],
    &[("&", BinaryOp::BitAnd)],
    &[("===", BinaryOp::IdentityEq), ("!==", BinaryOp::IdentityNe)],
    &[("==", BinaryOp::Eq), ("!=", BinaryOp::NotEq)],
    &[
        ("<", BinaryOp::Lt),
        ("<=", BinaryOp::Le),
        (">", BinaryOp::Gt),
        (">=", BinaryOp::Ge),
    ],
    &[("<<", BinaryOp::ShiftLeft), (">>", BinaryOp::ShiftRight)],
    &[("+", BinaryOp::Add), ("-", BinaryOp::Sub)],
    &[
        ("*", BinaryOp::Mul),
        ("/", BinaryOp::Div),
        ("%", BinaryOp::Mod),
    ],
];

/// `in` membership is not a [`BINARY_LEVELS`] row. It binds looser than
/// the string-match operators and tighter than `&` (clause 4.5).
const IN_LEVEL: usize = 6;

/// `~` / `!~` string match is not a [`BINARY_LEVELS`] row. It binds looser
/// than `===` / `!==` and tighter than `in` (clause 4.5, Table 12).
const MATCH_LEVEL: usize = 7;

/// `is [not] a <type>` is not a [`BINARY_LEVELS`] row: its right operand
/// is a type, not an expression. It binds tighter than `==` / `!=` and
/// looser than the comparisons (clause 4.5, Table 12).
const ISA_LEVEL: usize = 10;

/// The [`BINARY_LEVELS`] row for a parse level, if the level is a plain
/// binary-operator row. Two special levels (`in` at [`IN_LEVEL`] and
/// string match at [`MATCH_LEVEL`]) sit between rows 5 and 6, and `is
/// [not] a` ([`ISA_LEVEL`]) between rows 7 and 8, so levels above a
/// special level shift by the number of specials passed.
fn binary_row_for(level: usize) -> Option<&'static [(&'static str, BinaryOp)]> {
    match level {
        0..=5 => BINARY_LEVELS.get(level).copied(),
        8..=9 => BINARY_LEVELS.get(level - 2).copied(),
        11..=14 => BINARY_LEVELS.get(level - 3).copied(),
        _ => None,
    }
}

/// Outcome of parsing one source file: whatever declarations parsed, plus
/// every diagnostic found (syntax and well-formedness).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseResult {
    pub declarations: Vec<Declaration>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Parse e source text. Never panics on bad input; problems are reported as
/// diagnostics.
pub fn parse(source: &str) -> ParseResult {
    let mut diagnostics = Vec::new();
    let tokens = match lexer::tokenize(source) {
        Ok(tokens) => tokens,
        Err(err) => {
            let end = Position {
                byte: err.position.byte + 1,
                line: err.position.line,
                column: err.position.column + 1,
            };
            diagnostics.push(Diagnostic::new("E0001", err.message, err.position, end));
            return ParseResult {
                declarations: Vec::new(),
                diagnostics,
            };
        }
    };
    let eof = tokens.last().map(|t| t.end).unwrap_or(Position {
        byte: source.len(),
        line: 1,
        column: 1,
    });
    let mut parser = Parser::new(tokens, eof);
    let declarations = parser.parse_program();
    diagnostics.extend(parser.diagnostics);
    diagnostics.extend(crate::check::check_declarations(&declarations));
    ParseResult {
        declarations,
        diagnostics,
    }
}

/// Parse without the well-formedness pass — for multi-file drivers that
/// merge declarations first and check the merged set once.
pub fn parse_unchecked(source: &str) -> ParseResult {
    let mut diagnostics = Vec::new();
    let tokens = match lexer::tokenize(source) {
        Ok(tokens) => tokens,
        Err(err) => {
            let end = Position {
                byte: err.position.byte + 1,
                line: err.position.line,
                column: err.position.column + 1,
            };
            diagnostics.push(Diagnostic::new("E0001", err.message, err.position, end));
            return ParseResult {
                declarations: Vec::new(),
                diagnostics,
            };
        }
    };
    let eof = tokens.last().map(|t| t.end).unwrap_or(Position {
        byte: source.len(),
        line: 1,
        column: 1,
    });
    let mut parser = Parser::new(tokens, eof);
    let declarations = parser.parse_program();
    diagnostics.extend(parser.diagnostics);
    ParseResult {
        declarations,
        diagnostics,
    }
}

/// Whether a number token denotes a real: a fraction, or an exponent whose
/// digits-before part is plain decimal (so `0xE5` stays an integer).
fn looks_like_real(text: &str) -> bool {
    if text.contains('.') {
        return true;
    }
    match text.find(['e', 'E']) {
        Some(pos) => !text[..pos].is_empty() && text[..pos].chars().all(|c| c.is_ascii_digit()),
        None => false,
    }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    eof: Position,
    diagnostics: Vec<Diagnostic>,
    /// Names currently defined by `#define` preprocessor directives,
    /// consulted by `#ifdef` / `#ifndef` (clause 21). Directive state
    /// follows source order.
    defined: BTreeSet<String>,
    /// `package` / `private` visibility prefix of the member being
    /// parsed (clause 22), recorded on the field, not enforced.
    member_visibility: Option<String>,
}

/// The included body of one preprocessor directive: members in member
/// position, statements in action position, or nothing (`#define`,
/// `#undef`, and excluded `#ifdef` / `#ifndef` branches).
enum HashBody {
    Members(Vec<Member>),
    Statements(Vec<Stmt>),
    Skipped,
}

impl Parser {
    fn new(tokens: Vec<Token>, eof: Position) -> Self {
        Self {
            tokens,
            pos: 0,
            eof,
            diagnostics: Vec::new(),
            defined: BTreeSet::new(),
            member_visibility: None,
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn peek_kind(&self) -> Option<&TokenKind> {
        self.peek().map(|t| &t.kind)
    }

    fn peek_second_kind(&self) -> Option<&TokenKind> {
        self.tokens.get(self.pos + 1).map(|t| &t.kind)
    }

    fn bump(&mut self) -> Token {
        let token = self.tokens[self.pos].clone();
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        token
    }

    fn last_end(&self) -> Position {
        self.tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.end)
            .unwrap_or(self.eof)
    }

    fn at_keyword(&self, keyword: &str) -> bool {
        matches!(self.peek_kind(), Some(TokenKind::Keyword(k)) if k == keyword)
    }

    fn at_identifier(&self) -> bool {
        matches!(self.peek_kind(), Some(TokenKind::Identifier(_)))
    }

    fn at_punct(&self, symbol: &str) -> bool {
        matches!(self.peek_kind(), Some(TokenKind::Punctuation(p)) if p == symbol)
    }

    fn eat_keyword(&mut self, keyword: &str) -> bool {
        if self.at_keyword(keyword) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn eat_punct(&mut self, symbol: &str) -> bool {
        if self.at_punct(symbol) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn current_span(&self) -> (Position, Position) {
        match self.peek() {
            Some(token) => (token.start, token.end),
            None => (self.eof, self.eof),
        }
    }

    fn error_here(&mut self, code: &'static str, message: &str) {
        let (start, end) = self.current_span();
        self.diagnostics
            .push(Diagnostic::new(code, message, start, end));
    }

    /// Skip tokens until just past a member terminator (`;`) or until a
    /// member/declaration boundary (`}`, or a top-level declaration
    /// keyword) is reached.
    fn recover_to_member_boundary(&mut self) {
        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Punctuation(p) if p == ";" => {
                    self.bump();
                    return;
                }
                TokenKind::Punctuation(p) if p == "}" => return,
                TokenKind::Keyword(k) if matches!(k.as_str(), "struct" | "unit" | "extend") => {
                    return;
                }
                _ => {
                    self.bump();
                }
            }
        }
    }

    /// Skip tokens until the next top-level declaration keyword.
    fn recover_to_declaration_boundary(&mut self) {
        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Keyword(k) if matches!(k.as_str(), "struct" | "unit" | "extend") => {
                    return;
                }
                _ => {
                    self.bump();
                }
            }
        }
    }

    /// Consume `symbol` or record `code`; `None` means the symbol was
    /// missing (the diagnostic is already recorded).
    fn expect_punct(&mut self, symbol: &str, code: &'static str, message: &str) -> Option<()> {
        if self.eat_punct(symbol) {
            Some(())
        } else {
            self.error_here(code, message);
            None
        }
    }

    /// Consume the identifier the caller has already checked with
    /// [`Self::at_identifier`].
    fn take_identifier(&mut self) -> (String, (Position, Position)) {
        let token = self.bump();
        let TokenKind::Identifier(name) = token.kind else {
            unreachable!("caller checked at_identifier");
        };
        (name, (token.start, token.end))
    }

    fn expect_identifier(
        &mut self,
        code: &'static str,
        message: &str,
    ) -> Option<(String, (Position, Position))> {
        if self.at_identifier() {
            Some(self.take_identifier())
        } else {
            self.error_here(code, message);
            None
        }
    }

    /// A word in identifier position that may also be a keyword spelling
    /// — template formals are conventionally named `<type>`, and `type`
    /// is a keyword (clause 8.1).
    fn expect_word(
        &mut self,
        code: &'static str,
        message: &str,
    ) -> Option<(String, (Position, Position))> {
        let (start, end) = self.current_span();
        match self.peek_kind() {
            Some(TokenKind::Identifier(_)) => Some(self.take_identifier()),
            Some(TokenKind::Keyword(word)) => {
                let word = word.clone();
                self.bump();
                Some((word, (start, end)))
            }
            _ => {
                self.error_here(code, message);
                None
            }
        }
    }

    /// A type name: an identifier, or the `global` keyword (`extend
    /// global` extends the root environment, as in Specman).
    fn expect_type_name(&mut self, message: &str) -> Option<(String, (Position, Position))> {
        if self.at_identifier() {
            return Some(self.take_identifier());
        }
        if self.peek_word_is("global") {
            let (start, end) = self.current_span();
            self.bump();
            return Some(("global".to_owned(), (start, end)));
        }
        self.error_here("E0102", message);
        None
    }

    fn parse_program(&mut self) -> Vec<Declaration> {
        let mut declarations: Vec<Declaration> = Vec::new();
        // `package` prefix of the next declaration (clause 22 visibility,
        // recorded not enforced).
        let mut packaged = false;
        while self.peek().is_some() {
            let keyword = match self.peek_kind() {
                Some(TokenKind::Keyword(k))
                    if matches!(k.as_str(), "struct" | "unit" | "extend" | "type") =>
                {
                    k.clone()
                }
                // File-level `define` constants, macro definitions and
                // `import` lists live in the global scope; they are
                // represented as members of an `extend global`
                // pseudo-declaration.
                Some(TokenKind::Keyword(k)) if k.as_str() == "import" => {
                    let (gstart, _) = self.current_span();
                    let Some(members) = self.parse_import() else {
                        continue;
                    };
                    let gend = self.last_end();
                    match declarations.last_mut() {
                        Some(last)
                            if last.kind == DeclarationKind::Extend && last.name == "global" =>
                        {
                            last.members.extend(members);
                        }
                        _ => declarations.push(Declaration {
                            kind: DeclarationKind::Extend,
                            name: "global".to_owned(),
                            name_span: (gstart, gstart),
                            underlying: None,
                            template_params: Vec::new(),
                            packaged: false,
                            like: None,
                            like_span: None,
                            like_type: None,
                            modifiers: Vec::new(),
                            members,
                            span: (gstart, gend),
                        }),
                    }
                    continue;
                }
                Some(TokenKind::Keyword(k)) if k.as_str() == "define" => {
                    let (gstart, _) = self.current_span();
                    let members = if self.peek_second_is_punct("<") {
                        self.parse_macro_definition().map(|m| vec![m])
                    } else {
                        self.parse_define().map(|m| vec![m])
                    };
                    let Some(members) = members else {
                        continue;
                    };
                    let gend = self.last_end();
                    match declarations.last_mut() {
                        Some(last)
                            if last.kind == DeclarationKind::Extend && last.name == "global" =>
                        {
                            last.members.extend(members);
                        }
                        _ => declarations.push(Declaration {
                            kind: DeclarationKind::Extend,
                            name: "global".to_owned(),
                            name_span: (gstart, gstart),
                            underlying: None,
                            template_params: Vec::new(),
                            packaged: false,
                            like: None,
                            like_span: None,
                            like_type: None,
                            modifiers: Vec::new(),
                            members,
                            span: (gstart, gend),
                        }),
                    }
                    continue;
                }
                // `sequence NAME using item = T, created_driver = D,
                // created_kind = K;` — synthesizes the kind type (with
                // MAIN), the sequencer unit (item_done event +
                // get_next_item) and the sequence struct; execution-side
                // sequence scheduling is a later milestone.
                Some(TokenKind::Keyword(k)) if k.as_str() == "sequence" => {
                    if let Some(synthesized) = self.parse_sequence_declaration() {
                        declarations.extend(synthesized);
                    }
                    continue;
                }
                // `template (struct|unit) ...` (clause 8).
                Some(TokenKind::Keyword(k)) if k.as_str() == "template" => {
                    if let Some(mut declaration) = self.parse_template_declaration() {
                        declaration.packaged = packaged;
                        packaged = false;
                        declarations.push(declaration);
                    } else {
                        packaged = false;
                    }
                    continue;
                }
                // `package` visibility prefix of the following
                // declaration (`package template struct ...`), recorded.
                Some(TokenKind::Keyword(k))
                    if k == "package"
                        && matches!(
                            self.peek_second_kind(),
                            Some(TokenKind::Keyword(k))
                                if matches!(
                                    k.as_str(),
                                    "template" | "struct" | "unit" | "extend" | "type"
                                )
                        ) =>
                {
                    self.bump();
                    packaged = true;
                    continue;
                }
                // File-level preprocessor directives: only `#define` /
                // `#undef` (clause 21.2 lists the conditional forms as
                // statement, struct-member and action positions, not
                // module level).
                Some(TokenKind::Punctuation(p)) if p == "#" => {
                    match self.peek_second_kind() {
                        Some(TokenKind::Keyword(k)) if k == "define" => {
                            self.parse_hash_directive(true);
                        }
                        Some(TokenKind::Identifier(w)) if w == "undef" => {
                            self.parse_hash_directive(true);
                        }
                        _ => {
                            self.error_here(
                                "E0117",
                                "expected '#define' or '#undef' at file level; the conditional directives are statement or struct-member forms",
                            );
                            self.recover_to_declaration_boundary();
                        }
                    }
                    continue;
                }
                _ => {
                    self.error_here(
                        "E0101",
                        "expected 'struct', 'unit', 'extend' or 'type' declaration",
                    );
                    self.recover_to_declaration_boundary();
                    continue;
                }
            };
            let declaration = if keyword == "type" {
                let mut declaration = self.parse_type_declaration();
                if let Some(d) = declaration.as_mut() {
                    d.packaged = packaged;
                }
                packaged = false;
                declaration
            } else {
                let mut declaration = self.parse_declaration(&keyword);
                if let Some(d) = declaration.as_mut() {
                    d.packaged = packaged;
                }
                packaged = false;
                declaration
            };
            if let Some(declaration) = declaration {
                declarations.push(declaration);
            }
        }
        declarations
    }

    /// `sequence NAME using item = T, created_driver = D, created_kind
    /// = K;` — the specman sequence-declaration macro in source form.
    /// Synthesizes the declaration trio the rest of the language then
    /// sees: `type K: [MAIN];`, `unit D { event item_done;
    /// get_next_item(): T is empty; }` and `struct NAME;`. The sequence
    /// execution engine (do/handshake scheduling) is a later milestone.
    fn parse_sequence_declaration(&mut self) -> Option<Vec<Declaration>> {
        let start = self.current_span().0;
        self.bump(); // 'sequence'
        let (name, name_span) = self.expect_identifier("E0102", "expected the sequence name")?;
        self.expect_keyword_word("using")?;
        let mut item: Option<String> = None;
        let mut driver: Option<String> = None;
        let mut kind: Option<String> = None;
        loop {
            let option = self.take_word()?;
            self.expect_punct("=", "E0108", "expected '=' after the sequence option")?;
            let (value, _) = self.expect_type_name("expected the sequence option value")?;
            match option.as_str() {
                "item" => item = Some(value),
                "created_driver" => driver = Some(value),
                "created_kind" => kind = Some(value),
                _ => {
                    self.error_here("E0102", &format!("unknown sequence option '{option}'"));
                    return None;
                }
            }
            if !self.eat_punct(",") {
                break;
            }
        }
        self.expect_punct(";", "E0105", "expected ';' after the sequence declaration")?;
        let end = self.last_end();
        let Some(item) = item else {
            self.error_here("E0102", "the 'item' sequence option is required");
            return None;
        };
        let driver = driver.unwrap_or_else(|| format!("{name}_sequencer"));
        let kind = kind.unwrap_or_else(|| format!("{name}_kind_t"));
        let mut members = Vec::new();
        for option in ["created_kind", "created_driver", "item"] {
            let value = match option {
                "created_kind" => &kind,
                "created_driver" => &driver,
                _ => &item,
            };
            members.push(Member::Define {
                name: format!("{name}.{option}"),
                value: Some(Expr::StringLiteral {
                    value: value.clone(),
                    span: (start, end),
                }),
                span: (start, end),
            });
        }
        members.push(Member::Enumeral(crate::ast::Enumeral {
            name: "MAIN".to_owned(),
            name_span: (start, start),
            value: None,
        }));
        Some(vec![
            Declaration {
                kind: DeclarationKind::Type,
                name: kind,
                name_span: (start, start),
                underlying: None,
                template_params: Vec::new(),
                packaged: false,
                like: None,
                like_span: None,
                like_type: None,
                modifiers: Vec::new(),
                members: vec![members.pop().expect("MAIN enumeral")],
                span: (start, end),
            },
            Declaration {
                kind: DeclarationKind::Unit,
                name: driver,
                name_span: (start, start),
                underlying: None,
                template_params: Vec::new(),
                packaged: false,
                like: None,
                like_span: None,
                like_type: None,
                modifiers: Vec::new(),
                members: vec![
                    Member::Event(crate::ast::EventDecl {
                        name: "item_done".to_owned(),
                        name_span: (start, start),
                        trigger: None,
                        span: (start, end),
                    }),
                    Member::Method(crate::ast::MethodDecl {
                        name: "get_next_item".to_owned(),
                        name_span: (start, start),
                        params: Vec::new(),
                        return_type: Some(TypeExpr::Named {
                            name: item,
                            span: (start, start),
                        }),
                        tcm_event: None,
                        modifier: None,
                        body: Vec::new(),
                        span: (start, end),
                    }),
                ],
                span: (start, end),
            },
            Declaration {
                kind: DeclarationKind::Struct,
                name,
                name_span,
                underlying: None,
                template_params: Vec::new(),
                packaged: false,
                like: None,
                like_span: None,
                like_type: None,
                modifiers: Vec::new(),
                members,
                span: (start, end),
            },
        ])
    }

    /// `template (struct|unit) name of <params> [like base] { members };`
    /// (clause 8.1). The parameters are recorded; instantiation
    /// (monomorphization) is a later milestone. A parameterized base
    /// type (`like container of <type>`) lands in `like` (the base
    /// name) and `like_type` (the full expression). Template `extend`
    /// (clause 8.2) is not parsed yet.
    fn parse_template_declaration(&mut self) -> Option<Declaration> {
        let start = self.current_span().0;
        self.bump(); // 'template'
        let kind = match self.peek_kind() {
            Some(TokenKind::Keyword(k)) if k == "struct" => DeclarationKind::Struct,
            Some(TokenKind::Keyword(k)) if k == "unit" => DeclarationKind::Unit,
            _ => {
                self.error_here(
                    "E0101",
                    "expected 'struct' or 'unit' after 'template' (template extend is not supported yet)",
                );
                self.recover_to_declaration_boundary();
                return None;
            }
        };
        self.bump();
        let (name, name_span) = self.expect_identifier("E0102", "expected the template name")?;
        self.expect_keyword_word("of")?;
        let mut template_params = Vec::new();
        if self.eat_punct("(") {
            loop {
                template_params.push(self.parse_template_param()?);
                if !self.eat_punct(",") {
                    break;
                }
            }
            self.expect_punct(
                ")",
                "E0110",
                "expected ')' to close the template parameters",
            )?;
        } else {
            template_params.push(self.parse_template_param()?);
        }
        let mut like = None;
        let mut like_span = None;
        let mut like_type = None;
        if self.eat_keyword("like") {
            let base = self.parse_type()?;
            like_span = Some(base.span());
            like = match &base {
                TypeExpr::Named { name, .. } | TypeExpr::TemplateRef { name, .. } => {
                    Some(name.clone())
                }
                _ => None,
            };
            like_type = Some(base);
        }
        if self
            .expect_punct("{", "E0106", "expected '{' to open the declaration body")
            .is_none()
        {
            self.recover_to_declaration_boundary();
            return Some(Declaration {
                kind,
                name,
                name_span,
                underlying: None,
                template_params,
                packaged: false,
                like,
                like_span,
                like_type,
                modifiers: Vec::new(),
                members: Vec::new(),
                span: (start, self.eof),
            });
        }
        let members = self.parse_member_list();
        self.eat_punct(";");
        let end = self.last_end();
        Some(Declaration {
            kind,
            name,
            name_span,
            underlying: None,
            template_params,
            packaged: false,
            like,
            like_span,
            like_type,
            modifiers: Vec::new(),
            members,
            span: (start, end),
        })
    }

    /// One `<[tag']category [= default]>` template parameter.
    fn parse_template_param(&mut self) -> Option<crate::ast::TemplateParam> {
        self.expect_punct("<", "E0106", "expected '<' to open the template parameter")?;
        let (name, name_span) = self.expect_word("E0102", "expected the parameter name")?;
        let mut name = name;
        if self.eat_punct("'") {
            let (category, _) = self.expect_word("E0102", "expected the parameter category")?;
            name.push('\'');
            name.push_str(&category);
        }
        self.expect_punct(">", "E0107", "expected '>' to close the template parameter")?;
        let default = if self.eat_punct("=") {
            Some(self.parse_type()?)
        } else {
            None
        };
        Some(crate::ast::TemplateParam {
            name,
            name_span,
            default,
        })
    }

    /// `type name: [ENUMERAL [= expr] , ..];` or the clause 5 scalar
    /// form `type name: <type>;` (`type addr_t: int (bits: 8);`) — the
    /// underlying representation is recorded; distinct-type semantics
    /// over it are a later milestone.
    fn parse_type_declaration(&mut self) -> Option<Declaration> {
        let start = self.current_span().0;
        self.bump(); // 'type'
        let (name, name_span) = self.expect_identifier("E0102", "expected type name")?;
        self.expect_punct(":", "E0103", "expected ':' after the type name")?;
        if !self.at_punct("[") {
            // Scalar/list type declaration: record the representation.
            let underlying = self.parse_type()?;
            self.expect_punct(";", "E0105", "expected ';' after the type declaration")?;
            let end = self.last_end();
            return Some(Declaration {
                kind: DeclarationKind::Type,
                name,
                name_span,
                underlying: Some(underlying),
                template_params: Vec::new(),
                packaged: false,
                like: None,
                like_span: None,
                like_type: None,
                modifiers: Vec::new(),
                members: Vec::new(),
                span: (start, end),
            });
        }
        if self
            .expect_punct("[", "E0106", "expected '[' to open the enumeral list")
            .is_none()
        {
            self.recover_to_declaration_boundary();
            return Some(Declaration {
                kind: DeclarationKind::Type,
                name,
                name_span,
                underlying: None,
                template_params: Vec::new(),
                packaged: false,
                like: None,
                like_span: None,
                like_type: None,
                modifiers: Vec::new(),
                members: Vec::new(),
                span: (start, self.eof),
            });
        }
        let mut members = Vec::new();
        loop {
            if self.at_punct("]") || self.peek().is_none() {
                break;
            }
            let Some((enumeral_name, enumeral_span)) =
                self.expect_identifier("E0102", "expected an enumeral name")
            else {
                break;
            };
            let value = if self.eat_punct("=") {
                Some(self.parse_expr()?)
            } else {
                None
            };
            members.push(Member::Enumeral(Enumeral {
                name: enumeral_name,
                name_span: enumeral_span,
                value,
            }));
            if !self.eat_punct(",") {
                break;
            }
        }
        if self
            .expect_punct("]", "E0107", "expected ']' to close the enumeral list")
            .is_none()
        {
            self.recover_to_declaration_boundary();
            return Some(Declaration {
                kind: DeclarationKind::Type,
                name,
                name_span,
                underlying: None,
                template_params: Vec::new(),
                packaged: false,
                like: None,
                like_span: None,
                like_type: None,
                modifiers: Vec::new(),
                members,
                span: (start, self.last_end()),
            });
        }
        // Optional storage width: `type e: [A, B] (bits: 4);` — accepted
        // for syntax compatibility; scalar sizing of enumerals is not
        // modeled yet.
        if self.eat_punct("(") {
            self.eat_keyword("bits");
            self.expect_punct(":", "E0103", "expected ':' after 'bits'")?;
            self.parse_expr()?;
            self.expect_punct(")", "E0110", "expected ')' after the width")?;
        }
        if self
            .expect_punct(";", "E0105", "expected ';' after the type declaration")
            .is_none()
        {
            self.recover_to_declaration_boundary();
            return Some(Declaration {
                kind: DeclarationKind::Type,
                name,
                name_span,
                underlying: None,
                template_params: Vec::new(),
                packaged: false,
                like: None,
                like_span: None,
                like_type: None,
                modifiers: Vec::new(),
                members,
                span: (start, self.last_end()),
            });
        }
        let end = self.last_end();
        Some(Declaration {
            kind: DeclarationKind::Type,
            name,
            name_span,
            underlying: None,
            template_params: Vec::new(),
            packaged: false,
            like: None,
            like_span: None,
            like_type: None,
            modifiers: Vec::new(),
            members,
            span: (start, end),
        })
    }

    fn parse_declaration(&mut self, keyword: &str) -> Option<Declaration> {
        let start = self.current_span().0;
        let kind = match keyword {
            "struct" => DeclarationKind::Struct,
            "unit" => DeclarationKind::Unit,
            "extend" => DeclarationKind::Extend,
            _ => unreachable!("caller validated the keyword"),
        };
        self.bump();
        // When-modifier prefixes qualify the extended subtype (`extend
        // imm cpu_instr_s`, `extend ACTIVE TX uart_agent_u`). The
        // declared name is always the LAST word before `{` or `like`,
        // so every preceding word is a modifier (`value'id` pairs are
        // flattened to their two words). Modifiers are recorded but
        // when-scoping of extensions is not modeled yet.
        let mut modifiers = Vec::new();
        let (name, name_span) = if kind == DeclarationKind::Extend && self.at_identifier() {
            let mut words: Vec<Vec<(String, (Position, Position))>> = Vec::new();
            while self.at_identifier() {
                let Some(Token {
                    kind: word_kind,
                    start: ws,
                    end: we,
                }) = self.peek().cloned()
                else {
                    break;
                };
                let word = match word_kind {
                    TokenKind::Identifier(text) => text,
                    TokenKind::Keyword(text) if text == "global" => text,
                    _ => break,
                };
                self.bump();
                let mut parts = vec![(word, (ws, we))];
                if self.eat_punct("'") {
                    let (id, id_span) =
                        self.expect_identifier("E0102", "expected an id after the modifier quote")?;
                    parts.push((id, id_span));
                }
                words.push(parts);
            }
            // The final word run is the type name.
            let last = words.pop().expect("checked at_identifier above");
            for parts in words {
                modifiers.extend(parts.into_iter().map(|(word, _)| word));
            }
            let (name, name_span) = last.into_iter().next().expect("non-empty run");
            (name, name_span)
        } else {
            self.expect_type_name("expected declaration name")?
        };
        // Clause 4.3.1.1: extensions of the root `global` type are reserved
        // for the runtime; user code shall not write `extend global`. The
        // parser's own `extend global` pseudo-declaration (the holder of
        // file-level `define`/`import` members) is built in
        // `parse_program` and never passes through here.
        if kind == DeclarationKind::Extend && name == "global" {
            self.diagnostics.push(Diagnostic::new(
                "E0116",
                "cannot extend 'global': extensions of the root type are reserved for the runtime",
                name_span.0,
                name_span.1,
            ));
            self.recover_to_declaration_boundary();
            return None;
        }
        let mut like = None;
        let mut like_span = None;
        let mut like_type = None;
        if self.eat_keyword("like") {
            // The base may be a template instance (`like linked_list of
            // packet`, clause 8.3); the full type is recorded alongside
            // the base name.
            let base = self.parse_type()?;
            like_span = Some(base.span());
            like = match &base {
                TypeExpr::Named { name, .. } | TypeExpr::TemplateRef { name, .. } => {
                    Some(name.clone())
                }
                _ => None,
            };
            like_type = Some(base);
        }
        if self
            .expect_punct("{", "E0106", "expected '{' to open the declaration body")
            .is_none()
        {
            self.recover_to_declaration_boundary();
            return Some(Declaration {
                kind,
                name,
                name_span,
                underlying: None,
                template_params: Vec::new(),
                packaged: false,
                like_type,
                like,
                like_span,
                modifiers,
                members: Vec::new(),
                span: (start, self.eof),
            });
        }
        let members = self.parse_member_list();
        // e convention writes the closing brace as `};`; the semicolon is
        // accepted but not required.
        self.eat_punct(";");
        let end = self.last_end();
        Some(Declaration {
            kind,
            name,
            name_span,
            underlying: None,
            template_params: Vec::new(),
            packaged: false,
            like_type,
            like,
            like_span,
            modifiers,
            members,
            span: (start, end),
        })
    }

    /// Members up to (and including) the closing `}` of the enclosing
    /// declaration or `when` body.
    fn parse_member_list(&mut self) -> Vec<Member> {
        let mut members = Vec::new();
        loop {
            if self.peek().is_none() {
                self.error_here("E0107", "expected '}' to close the declaration body");
                break;
            }
            if self.at_punct("}") {
                self.bump();
                break;
            }
            if let Some(mut parsed) = self.parse_member() {
                members.append(&mut parsed);
            }
        }
        members
    }

    /// One member often produces several AST members (`a, b: uint;` is two
    /// fields), hence the vector.
    fn parse_member(&mut self) -> Option<Vec<Member>> {
        // `package` / `private` visibility prefix of a member
        // (`package items: …`, `private mset: …`, clause 22) — recorded
        // on the field, not enforced. The prefix is consumed by the
        // field parser one level down.
        if (self.peek_word_is("package") || self.peek_word_is("private"))
            && !matches!(self.peek_second_kind(), Some(TokenKind::Punctuation(p)) if p == "(")
        {
            let word = match self.peek_kind() {
                Some(TokenKind::Keyword(word)) | Some(TokenKind::Identifier(word)) => word.clone(),
                _ => unreachable!("peek_word_is matched"),
            };
            self.bump();
            self.member_visibility = Some(word);
            let members = self.parse_member();
            self.member_visibility = None;
            return members;
        }
        match self.peek_kind() {
            Some(TokenKind::Keyword(k)) if k == "import" => self.parse_import(),
            Some(TokenKind::Keyword(k)) if k == "define" => self.parse_define().map(|m| vec![m]),
            Some(TokenKind::Identifier(_)) => {
                if matches!(self.peek_second_kind(), Some(TokenKind::Punctuation(p)) if p == "(") {
                    self.parse_method().map(|m| vec![m])
                } else if self.peek_is_port_declaration() {
                    self.parse_port().map(|p| vec![p])
                } else {
                    self.parse_fields()
                }
            }
            Some(TokenKind::Keyword(k)) if k == "keep" => {
                self.parse_keep().map(|c| vec![Member::Constraint(c)])
            }
            Some(TokenKind::Keyword(k)) if k == "when" => self.parse_when(),
            Some(TokenKind::Keyword(k)) if k == "event" => self.parse_event().map(|e| vec![e]),
            Some(TokenKind::Keyword(k)) if k == "on" => {
                self.parse_on().map(|o| vec![Member::On(o)])
            }
            Some(TokenKind::Keyword(k)) if k == "cover" => {
                self.parse_cover_group().map(|c| vec![Member::Cover(c)])
            }
            Some(TokenKind::Keyword(k)) if k == "expect" || k == "assume" => {
                self.parse_expect().map(|e| vec![Member::Expect(e)])
            }
            Some(TokenKind::Punctuation(p)) if p == "#" => self.parse_hash_members(),
            // The explicit `method name() …` spelling; the bare
            // `name() …` init style is handled by the identifier arm.
            Some(TokenKind::Keyword(k)) if k == "method" => self.parse_method().map(|m| vec![m]),
            // A keyword-spelled method name (`set(index: uint, …)` —
            // `set` is a Table 8 keyword). The `(` disambiguates from
            // the statement/member keywords handled above.
            Some(TokenKind::Keyword(_)) if matches!(self.peek_second_kind(), Some(TokenKind::Punctuation(p)) if p == "(") => {
                self.parse_method().map(|m| vec![m])
            }
            // A keyword-spelled field name (`item: <type>;`, `prev:
            // llist_node of <type>;` — Table 8 keywords). The `:` or
            // dimension bracket disambiguates.
            Some(TokenKind::Keyword(_))
                if matches!(self.peek_second_kind(), Some(TokenKind::Punctuation(p)) if p == ":")
                    || self.peek_second_is_word("is") =>
            {
                self.parse_fields()
            }
            // `%name` physical / `!name` do-not-generate field prefixes.
            Some(TokenKind::Punctuation(p)) if p == "%" || p == "!" => self.parse_fields(),
            _ => {
                self.error_here(
                    "E0113",
                    "expected a field, method, 'when' or 'keep' declaration (other members are not supported yet)",
                );
                self.recover_to_member_boundary();
                None
            }
        }
    }

    /// `import a, b.c;` — records the imported module names; cross-file
    /// resolution is a later milestone.
    fn parse_import(&mut self) -> Option<Vec<Member>> {
        let start = self.current_span().0;
        self.bump(); // 'import'
        let mut names = Vec::new();
        loop {
            let (name, _) =
                self.expect_identifier("E0102", "expected a module name after 'import'")?;
            names.push(name);
            if !self.eat_punct(",") {
                break;
            }
        }
        self.expect_punct(";", "E0105", "expected ';' after the import list")?;
        let end = self.last_end();
        Some(vec![Member::Import {
            names,
            span: (start, end),
        }])
    }

    /// `define name value;` — a compile-time constant.
    fn parse_define(&mut self) -> Option<Member> {
        let start = self.current_span().0;
        self.bump(); // 'define'
        let (name, _name_span) =
            self.expect_identifier("E0102", "expected a name after 'define'")?;
        let value = if self.at_punct(";") {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect_punct(";", "E0105", "expected ';' after the define value")?;
        let end = self.last_end();
        Some(Member::Define {
            name,
            value,
            span: (start, end),
        })
    }

    /// `define <name['category]> "match expression" as [computed] { body };`
    /// — a macro definition (clause 15.2/15.3), recorded uninterpreted:
    /// the match-expression grammar (clause 15.4) and the expansion
    /// engine are later milestones.
    fn parse_macro_definition(&mut self) -> Option<Member> {
        let start = self.current_span().0;
        self.bump(); // 'define'
        self.expect_punct("<", "E0106", "expected '<' to open the macro name")?;
        let (name, name_span) = self.expect_identifier("E0102", "expected the macro name")?;
        let category = if self.eat_punct("'") {
            let (category, _) =
                self.expect_identifier("E0102", "expected the macro category after \"'\"")?;
            Some(category)
        } else {
            None
        };
        self.expect_punct(">", "E0107", "expected '>' to close the macro name")?;
        let pattern = match self.peek_kind() {
            Some(TokenKind::StringLiteral(text)) => {
                let text = text.clone();
                self.bump();
                text
            }
            _ => {
                self.error_here("E0118", "expected the match expression string");
                return None;
            }
        };
        self.expect_keyword_word("as")?;
        // `computed` is not a reserved word in the lexer's keyword table.
        let computed = self.eat_word("computed");
        self.expect_punct("{", "E0106", "expected '{' to open the macro body")?;
        let body = self.capture_braced_body();
        self.eat_punct(";");
        let end = self.last_end();
        Some(Member::Macro(crate::ast::MacroDecl {
            name,
            name_span,
            category,
            pattern,
            computed,
            body,
            span: (start, end),
        }))
    }

    /// Whether the token after the current one is the given punctuation.
    fn peek_second_is_punct(&self, symbol: &str) -> bool {
        matches!(self.peek_second_kind(), Some(TokenKind::Punctuation(p)) if p == symbol)
    }

    /// Whether the token after the current one is the given word, as a
    /// keyword or an identifier.
    fn peek_second_is_word(&self, word: &str) -> bool {
        matches!(
            self.peek_second_kind(),
            Some(TokenKind::Identifier(w)) if w == word
        ) || matches!(
            self.peek_second_kind(),
            Some(TokenKind::Keyword(w)) if w == word
        )
    }

    /// Collect the raw tokens up to and including the `}` matching the
    /// already-consumed `{`. Macro bodies are recorded, never parsed —
    /// a simple macro's template may contain constructs the parser does
    /// not know (the macro teaches the parser about them).
    fn capture_braced_body(&mut self) -> Vec<Token> {
        let mut captured = Vec::new();
        let mut depth = 1usize;
        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Punctuation(p) if p == "{" => {
                    depth += 1;
                    captured.push(self.bump());
                }
                TokenKind::Punctuation(p) if p == "}" => {
                    let token = self.bump();
                    depth -= 1;
                    if depth == 0 {
                        return captured;
                    }
                    captured.push(token);
                }
                _ => captured.push(self.bump()),
            }
        }
        self.error_here("E0107", "expected '}' to close the macro body");
        captured
    }

    /// `cover <event> is { [items, crosses, transitions, option
    /// entries] };` (clause 14.1) or `cover <event> is empty;`. Items
    /// parse structurally (`item name [: type] [= expr] [using …];`);
    /// every `using` option clause and non-item body entry is recorded
    /// as raw tokens — the option semantics belong to the coverage data
    /// model, a later milestone. Nothing is collected at run time yet.
    fn parse_cover_group(&mut self) -> Option<crate::ast::CoverGroupDecl> {
        let start = self.current_span().0;
        self.bump(); // 'cover'
        let event = self.parse_expr()?;
        // The group identifies by its sampling event's path.
        let name = render_expr_path(&event).unwrap_or_default();
        let name_span = event.span();
        self.expect_keyword_word("is")?;
        // `cover ... is also { .. }` extends the group (clause 14.5).
        let is_also = self.eat_keyword("also");
        if self.eat_keyword("empty") {
            self.expect_stmt_semi("expected ';' after the cover group")?;
            let end = self.last_end();
            return Some(crate::ast::CoverGroupDecl {
                name,
                name_span,
                event,
                empty: true,
                is_also,
                items: Vec::new(),
                options: Vec::new(),
                span: (start, end),
            });
        }
        self.expect_punct("{", "E0106", "expected '{' to open the cover group body")?;
        let mut items = Vec::new();
        let mut options = Vec::new();
        while !self.at_punct("}") && self.peek().is_some() {
            if self.at_punct(";") {
                // A stray empty statement (`;;`) is tolerated.
                self.bump();
                continue;
            }
            if self.at_keyword("item") || self.at_keyword("cross") || self.at_keyword("transition")
            {
                let kind = if self.at_keyword("item") {
                    crate::ast::CoverItemKind::Item
                } else if self.at_keyword("cross") {
                    crate::ast::CoverItemKind::Cross
                } else {
                    crate::ast::CoverItemKind::Transition
                };
                self.bump(); // kind keyword
                let mut names = Vec::new();
                loop {
                    let (name, name_span) =
                        self.expect_word("E0102", "expected the cover item name")?;
                    names.push((name, name_span));
                    if !self.eat_punct(",") {
                        break;
                    }
                }
                let mut ty = None;
                let mut expr = None;
                if kind == crate::ast::CoverItemKind::Item && self.eat_punct(":") {
                    ty = Some(self.parse_type()?);
                    if self.eat_punct("=") {
                        expr = Some(self.parse_expr()?);
                    }
                }
                // The `using ...` option clause runs to the statement's
                // `;` and is recorded verbatim; braced option blocks
                // (`ranges = { range(...); range(...); }`) are skipped
                // with depth counting so inner semicolons do not end the
                // clause.
                let mut using = Vec::new();
                if self.eat_keyword("using") {
                    let mut depth = 0usize;
                    while let Some(token) = self.peek() {
                        match &token.kind {
                            TokenKind::Punctuation(p) if p == ";" && depth == 0 => break,
                            TokenKind::Punctuation(p) if p == "{" => {
                                depth += 1;
                                using.push(self.bump());
                            }
                            TokenKind::Punctuation(p) if p == "}" => {
                                depth = depth.saturating_sub(1);
                                using.push(self.bump());
                            }
                            _ => using.push(self.bump()),
                        }
                    }
                }
                self.expect_stmt_semi("expected ';' after the cover item")?;
                let end = self.last_end();
                items.push(crate::ast::CoverItemDecl {
                    kind,
                    names,
                    ty,
                    expr,
                    using,
                    span: (start, end),
                });
            } else {
                // A group-level option entry (`per_instance = TRUE;`,
                // `weight = ...;`) — recorded verbatim, not interpreted.
                let mut entry = Vec::new();
                while let Some(token) = self.peek() {
                    if matches!(&token.kind, TokenKind::Punctuation(p) if p == ";") {
                        break;
                    }
                    if matches!(&token.kind, TokenKind::Punctuation(p) if p == "}") {
                        break;
                    }
                    entry.push(self.bump());
                }
                self.eat_punct(";");
                if entry.is_empty() {
                    // No progress — skip a token to avoid looping.
                    if self.peek().is_some() && !self.at_punct("}") {
                        self.bump();
                    } else {
                        break;
                    }
                }
                options.push(entry);
            }
        }
        self.expect_punct("}", "E0107", "expected '}' to close the cover group body")?;
        self.expect_stmt_semi("expected ';' after the cover group")?;
        let end = self.last_end();
        Some(crate::ast::CoverGroupDecl {
            name,
            name_span,
            event,
            empty: false,
            is_also,
            items,
            options,
            span: (start, end),
        })
    }

    /// `expect <temporal-expression> [else <failure body>];` and the
    /// `assume` twin (clause 11.4). The temporal expression parses into
    /// the clause 12 grammar; evaluation over paths is the temporal
    /// engine, a later milestone. The failure body parses as ordinary
    /// actions.
    fn parse_expect(&mut self) -> Option<crate::ast::ExpectDecl> {
        let start = self.current_span().0;
        let kind = match self.peek_kind() {
            Some(TokenKind::Keyword(k)) if k == "expect" => crate::ast::ExpectKind::Expect,
            _ => crate::ast::ExpectKind::Assume,
        };
        self.bump(); // 'expect' / 'assume'
        let te = self.parse_temporal_expr()?;
        let mut else_body = Vec::new();
        // The one-statement else form consumes its own terminator; the
        // member-level `;` then only applies to the braced/no-else
        // forms.
        let mut semi_consumed_by_else = false;
        if self.eat_keyword("else") {
            if self.eat_punct("{") {
                else_body = self.parse_statements();
                self.expect_punct("}", "E0107", "expected '}' to close the 'else' body")?;
            } else if let Some(statement) = self.parse_statement() {
                else_body.push(statement);
                semi_consumed_by_else = true;
            }
        }
        if !semi_consumed_by_else {
            self.expect_stmt_semi("expected ';' after the expect member")?;
        }
        let end = self.last_end();
        Some(crate::ast::ExpectDecl {
            kind,
            te,
            else_body,
            span: (start, end),
        })
    }

    /// The clause 12 temporal-expression grammar. Precedence, loosest to
    /// tightest: `=>` (12.2.13), `or` (12.2.7), `and` (12.2.6), the
    /// binary sampling `@` (12.2.5), repeats and prefix operators, then
    /// the atoms — Table 37 is an image in the local copy, so this order
    /// follows the prose conventions (`or` looser than `and`, sampling
    /// binds over both, yield loosest as the expect shape requires).
    fn parse_temporal_expr(&mut self) -> Option<crate::ast::TemporalExpr> {
        let lhs = self.parse_te_or()?;
        if self.at_punct("=>") {
            self.bump();
            let rhs = self.parse_temporal_expr()?;
            return Some(crate::ast::TemporalExpr::Yield {
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            });
        }
        Some(lhs)
    }

    fn parse_te_or(&mut self) -> Option<crate::ast::TemporalExpr> {
        let mut lhs = self.parse_te_and()?;
        while self.eat_keyword("or") {
            let rhs = self.parse_te_and()?;
            lhs = crate::ast::TemporalExpr::Or(Box::new(lhs), Box::new(rhs));
        }
        Some(lhs)
    }

    fn parse_te_and(&mut self) -> Option<crate::ast::TemporalExpr> {
        let mut lhs = self.parse_te_sample()?;
        while self.eat_keyword("and") {
            let rhs = self.parse_te_sample()?;
            lhs = crate::ast::TemporalExpr::And(Box::new(lhs), Box::new(rhs));
        }
        Some(lhs)
    }

    /// The suffix sampling operator: `te @event` (12.2.5) — the operand
    /// is a bare dotted event path (`@clk`-style atoms are the unary
    /// operator, 12.2.4).
    fn parse_te_sample(&mut self) -> Option<crate::ast::TemporalExpr> {
        let mut te = self.parse_te_prefix()?;
        while self.at_punct("@") {
            self.bump();
            let (path, _) = self.parse_te_event_path()?;
            let event = crate::ast::TemporalExpr::EventRef { path };
            te = crate::ast::TemporalExpr::Sampled {
                te: Box::new(te),
                event: Box::new(event),
            };
        }
        Some(te)
    }

    /// Repeats and the prefix operators `not`, `fail`, `eventually`.
    /// Three repeat spellings (12.2.9–12.2.11): the fixed repeat
    /// `[n]*te` (a single bound, no `..`), the true-match repeat
    /// `~[l..h]*te` (`~`; a single bound is the exact count) and the
    /// first-match repeat `[l..h]*te` (a range without `~`, valid only
    /// as a sequence item).
    fn parse_te_prefix(&mut self) -> Option<crate::ast::TemporalExpr> {
        if self.eat_keyword("not") {
            let inner = self.parse_te_prefix()?;
            return Some(crate::ast::TemporalExpr::Not(Box::new(inner)));
        }
        if self.eat_keyword("fail") {
            let inner = self.parse_te_prefix()?;
            return Some(crate::ast::TemporalExpr::Fail(Box::new(inner)));
        }
        if self.eat_keyword("eventually") {
            let inner = self.parse_te_prefix()?;
            return Some(crate::ast::TemporalExpr::Eventually(Box::new(inner)));
        }
        let true_match = self.at_punct("~");
        let bracket_repeat = (self.at_punct("[") && !true_match)
            || (true_match && {
                let second = self.peek_second_kind();
                matches!(second, Some(TokenKind::Punctuation(p)) if p == "[")
            });
        if bracket_repeat {
            if true_match {
                self.bump(); // '~'
            }
            self.bump(); // '['
            // `~[..]*te` / `[..]*te` — an empty low bound is the
            // unbounded form (12.2.10, 12.2.11).
            let low = if self.at_punct("..") {
                None
            } else {
                Some(self.parse_expr()?)
            };
            let ranged = self.eat_punct("..");
            let high = if ranged {
                if self.at_punct("]") {
                    None
                } else {
                    Some(self.parse_expr()?)
                }
            } else {
                None
            };
            // A range without `~` is the first-match repeat (12.2.11);
            // a single bound is the fixed repeat (12.2.9) — or, under
            // `~`, the exact-count form of the true-match repeat
            // (12.2.10).
            let kind = match (true_match, ranged) {
                (true, _) => crate::ast::RepeatKind::TrueMatch,
                (false, true) => crate::ast::RepeatKind::FirstMatch,
                (false, false) => crate::ast::RepeatKind::Fixed,
            };
            let high = match (true_match, ranged, &low) {
                // `~[n]*te` is n consecutive successes: n..n.
                (true, false, Some(expr)) => Some(expr.clone()),
                _ => high,
            };
            self.expect_punct("]", "E0107", "expected ']' to close the repeat range")?;
            self.expect_punct("*", "E0106", "expected '*' after the repeat range")?;
            let te = self.parse_te_prefix()?;
            return Some(crate::ast::TemporalExpr::Repeat {
                kind,
                low,
                high,
                te: Box::new(te),
            });
        }
        self.parse_te_atom()
    }

    /// The temporal atoms: `cycle`, `true(exp)`, `change/rise/fall(exp)`,
    /// `@event`, `{ sequences }`, `( te )` (12.2.2–12.2.4, 12.2.8,
    /// 12.2.17).
    fn parse_te_atom(&mut self) -> Option<crate::ast::TemporalExpr> {
        if self.eat_keyword("cycle") {
            return Some(crate::ast::TemporalExpr::Cycle);
        }
        for (routine, edge) in [
            ("true", None::<crate::ast::EdgeRoutine>),
            ("change", Some(crate::ast::EdgeRoutine::Change)),
            ("rise", Some(crate::ast::EdgeRoutine::Rise)),
            ("fall", Some(crate::ast::EdgeRoutine::Fall)),
        ] {
            if self.peek_word_is(routine)
                && matches!(self.peek_second_kind(), Some(TokenKind::Punctuation(p)) if p == "(")
            {
                self.bump();
                self.expect_punct("(", "E0109", "expected '(' after the temporal routine")?;
                let expr = self.parse_expr()?;
                self.expect_punct(")", "E0110", "expected ')' after the sampled expression")?;
                return match edge {
                    None => Some(crate::ast::TemporalExpr::True { expr }),
                    Some(routine) => Some(crate::ast::TemporalExpr::EdgeSample { routine, expr }),
                };
            }
        }
        if self.at_punct("@") {
            self.bump();
            let (path, _) = self.parse_te_event_path()?;
            return Some(crate::ast::TemporalExpr::EventRef { path });
        }
        if self.eat_punct("{") {
            let mut items = vec![self.parse_temporal_expr()?];
            while self.eat_punct(";") {
                if self.at_punct("}") {
                    break;
                }
                items.push(self.parse_temporal_expr()?);
            }
            self.expect_punct("}", "E0107", "expected '}' to close the temporal sequence")?;
            return Some(crate::ast::TemporalExpr::Sequence { items });
        }
        if self.eat_punct("(") {
            let inner = self.parse_temporal_expr()?;
            self.expect_punct(
                ")",
                "E0110",
                "expected ')' to close the temporal expression",
            )?;
            return Some(crate::ast::TemporalExpr::Group(Box::new(inner)));
        }
        self.error_here(
            "E0120",
            "unsupported or unexpected temporal-expression construct",
        );
        None
    }

    /// `@`-event path: dotted words, each optionally `$`-suffixed
    /// (`@sys.cpuclk`, `@driver.p$`).
    fn parse_te_event_path(&mut self) -> Option<(Vec<String>, crate::ast::Span)> {
        let (start, _) = self.current_span();
        let (first, _) = self.expect_word("E0102", "expected the event name after '@'")?;
        let mut first = first;
        while self.at_punct("$") {
            self.bump();
            first.push('$');
        }
        let mut path = vec![first];
        let mut end = self.last_end();
        while self.eat_punct(".") {
            let (segment, _) = self.expect_word("E0102", "expected a path segment after '.'")?;
            let mut segment = segment;
            while self.at_punct("$") {
                self.bump();
                segment.push('$');
            }
            path.push(segment);
            end = self.last_end();
        }
        Some((path, (start, end)))
    }

    /// `on <event-path> { statements };` — an `on` struct member (clause
    /// 11.2). The event is a dotted path; the actions run when the event
    /// fires. Extending is a repeated declaration, so one `Member::On`
    /// per source block; composition keeps them in order. The clause's
    /// no-time-consuming-actions restriction is not enforced.
    fn parse_on(&mut self) -> Option<OnDecl> {
        let start = self.current_span().0;
        self.bump(); // 'on'
        let (segment, _) = self.expect_identifier("E0102", "expected an event name after 'on'")?;
        let mut path = vec![segment];
        while self.eat_punct(".") {
            let (segment, _) =
                self.expect_identifier("E0102", "expected a path segment after '.'")?;
            path.push(segment);
        }
        self.expect_punct("{", "E0106", "expected '{' to open the 'on' body")?;
        let body = self.parse_statements();
        self.expect_punct("}", "E0107", "expected '}' to close the 'on' body")?;
        self.eat_punct(";");
        let end = self.last_end();
        Some(OnDecl {
            path,
            body,
            span: (start, end),
        })
    }

    /// One `#define` / `#undef` / `#ifdef` / `#ifndef` preprocessor
    /// directive in member position (clause 21.2).
    fn parse_hash_members(&mut self) -> Option<Vec<Member>> {
        match self.parse_hash_directive(true)? {
            HashBody::Members(members) => Some(members),
            HashBody::Skipped => Some(Vec::new()),
            HashBody::Statements(_) => unreachable!("member position returns members"),
        }
    }

    /// The statement-position twin of [`Self::parse_hash_members`],
    /// called from the statement loop.
    fn parse_hash_statements(&mut self) -> Option<Vec<Stmt>> {
        match self.parse_hash_directive(false)? {
            HashBody::Statements(statements) => Some(statements),
            HashBody::Skipped => Some(Vec::new()),
            HashBody::Members(_) => unreachable!("statement position returns statements"),
        }
    }

    /// Consume one `#` directive. `#define NAME <replacement>;` records
    /// the flag only — the replacement-rule half of clause 21.3 (text
    /// substitution into later code) is a later milestone. An
    /// `#ifdef`/`#ifndef` body is parsed only when included; excluded
    /// code is skipped without parsing ("ignore it", clause 21.2). The
    /// body grammar follows the position: members in a struct body,
    /// statements in an action block.
    fn parse_hash_directive(&mut self, member_position: bool) -> Option<HashBody> {
        self.bump(); // '#'
        let directive = match self.peek_kind() {
            Some(TokenKind::Identifier(word))
                if word == "ifdef" || word == "ifndef" || word == "undef" =>
            {
                word.clone()
            }
            Some(TokenKind::Keyword(word)) if word == "define" => word.clone(),
            _ => {
                self.error_here(
                    "E0117",
                    "expected '#ifdef', '#ifndef', '#define' or '#undef'",
                );
                self.recover_to_member_boundary();
                return None;
            }
        };
        self.bump(); // directive word
        let (name, name_span) =
            self.expect_identifier("E0102", "expected a name after the directive")?;
        match directive.as_str() {
            "define" => {
                self.consume_hash_replacement(name_span.1.line);
                self.defined.insert(name);
                Some(HashBody::Skipped)
            }
            "undef" => {
                // `#undef NAME;` or the bare flag form terminated by the
                // line end, mirroring `#define`.
                if self
                    .peek()
                    .is_some_and(|t| t.start.line == name_span.1.line)
                {
                    self.expect_punct(";", "E0105", "expected ';' after '#undef'")?;
                }
                self.defined.remove(&name);
                Some(HashBody::Skipped)
            }
            _ => {
                let included = if directive == "ifdef" {
                    self.defined.contains(&name)
                } else {
                    !self.defined.contains(&name)
                };
                self.expect_punct("{", "E0106", "expected '{' to open the directive body")?;
                if !included {
                    self.skip_balanced_braces();
                    self.eat_punct(";");
                    return Some(HashBody::Skipped);
                }
                if member_position {
                    // parse_member_list consumes the closing `}`; the e
                    // convention writes `};`, so eat the semicolon too.
                    let members = self.parse_member_list();
                    self.eat_punct(";");
                    Some(HashBody::Members(members))
                } else {
                    let body = self.parse_statements();
                    self.expect_punct("}", "E0107", "expected '}' to close the directive body")?;
                    self.eat_punct(";");
                    Some(HashBody::Statements(body))
                }
            }
        }
    }

    /// Consume the `#define` replacement text without recording it
    /// (replacement rules are a later milestone). A replacement ends at
    /// `;` or at the end of the name's line — clause 21.3 allows line
    /// breaks in a replacement only behind a backslash, which the
    /// lexer does not produce.
    fn consume_hash_replacement(&mut self, name_line: usize) {
        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Punctuation(p) if p == ";" => {
                    self.bump();
                    return;
                }
                _ if token.start.line > name_line => return,
                _ => {
                    self.bump();
                }
            }
        }
    }

    /// Skip tokens after a directive's `{` with brace-depth counting, up
    /// to and including the matching `}`. Skipped code is ignored, not
    /// parsed (clause 21.2).
    fn skip_balanced_braces(&mut self) {
        let mut depth = 1usize;
        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Punctuation(p) if p == "{" => {
                    depth += 1;
                    self.bump();
                }
                TokenKind::Punctuation(p) if p == "}" => {
                    self.bump();
                    depth -= 1;
                    if depth == 0 {
                        return;
                    }
                }
                _ => {
                    self.bump();
                }
            }
        }
    }

    /// `when <modifier> [<modifier> ..] { members };` — a modifier is
    /// `value'id` or a bare `id` (the reference grammar's
    /// `struct_type_modifier`); the base-struct id, when present, is the
    /// last modifier.
    fn parse_when(&mut self) -> Option<Vec<Member>> {
        let start = self.current_span().0;
        self.bump(); // 'when'
        let mut modifiers = Vec::new();
        while self.at_identifier() {
            let (value, value_span) = self.take_identifier();
            // A missing identifier after `'` is recorded by
            // expect_identifier; keep parsing the when body with `id: None`.
            let id = if self.at_punct("'") {
                self.bump();
                self.expect_identifier(
                    "E0102",
                    "expected an identifier after \"'\" in the subtype modifier",
                )
            } else {
                None
            };
            modifiers.push(SubtypeModifier {
                value,
                value_span,
                id,
            });
        }
        if modifiers.is_empty() {
            self.error_here("E0102", "expected a subtype modifier after 'when'");
            self.recover_to_member_boundary();
            return None;
        }
        if self
            .expect_punct("{", "E0106", "expected '{' to open the 'when' body")
            .is_none()
        {
            self.recover_to_member_boundary();
            return Some(vec![Member::When(WhenDecl {
                modifiers,
                members: Vec::new(),
                span: (start, self.last_end()),
            })]);
        }
        let members = self.parse_member_list();
        let end = self.last_end();
        // e convention writes the closing brace as `};`; the semicolon is
        // accepted but not required.
        self.eat_punct(";");
        Some(vec![Member::When(WhenDecl {
            modifiers,
            members,
            span: (start, end),
        })])
    }

    /// `name, name: type;` — a name group declares one field per name.
    fn parse_fields(&mut self) -> Option<Vec<Member>> {
        // `%name` physical / `!name` do-not-generate prefixes.
        let physical = self.eat_punct("%");
        let no_gen = if !physical {
            self.eat_punct("!")
        } else {
            false
        };
        let (first, first_span) = self.expect_word("E0102", "expected field name")?;
        let mut names = vec![(first, first_span)];
        while self.eat_punct(",") {
            let (name, name_span) = self.expect_word("E0102", "expected field name after ','")?;
            names.push((name, name_span));
        }
        // Optional fixed-size dimension: `regs[4] : list of byte;`. The
        // list-length constraint is not modeled yet, so the dimension is
        // accepted and dropped.
        if self.at_punct("[") {
            self.bump();
            self.parse_expr()?;
            self.expect_punct("]", "E0107", "expected ']' after the field dimension")?;
        }
        // `name is instance;` — an implicitly typed field whose type is
        // the same-named struct (`verify is instance;`).
        if self.at_keyword("is") && !self.at_punct(":") {
            self.bump(); // 'is'
            if !self.eat_keyword("instance") && !self.eat_keyword("inline") {
                self.error_here("E0104", "expected 'instance' or 'inline' after 'is'");
                return None;
            }
            if !self.eat_punct(";") && !self.at_punct("}") {
                self.expect_punct(";", "E0105", "expected ';' after field declaration")?;
            }
            let _end = self.last_end();
            return Some(
                names
                    .into_iter()
                    .map(|(name, name_span)| {
                        Member::Field(Field {
                            name: name.clone(),
                            name_span,
                            ty: TypeExpr::Named {
                                name,
                                span: name_span,
                            },
                            physical: false,
                            no_gen: false,
                            visibility: self.member_visibility.clone(),
                        })
                    })
                    .collect(),
            );
        }
        self.expect_punct(":", "E0103", "expected ':' after field name")?;
        let ty = self.parse_type()?;
        // `is instance` / `is inline` suffixes are syntactic in this
        // subset.
        if self.eat_keyword("is") && !self.eat_keyword("instance") && !self.eat_keyword("inline") {
            self.error_here("E0104", "expected 'instance' or 'inline' after 'is'");
            return None;
        }
        // The final member of a body may omit the `;` (Specman accepts
        // both spellings).
        if !self.eat_punct(";") && !self.at_punct("}") {
            self.expect_punct(";", "E0105", "expected ';' after field declaration")?;
        }
        Some(
            names
                .into_iter()
                .map(|(name, name_span)| {
                    Member::Field(Field {
                        name,
                        name_span,
                        ty: ty.clone(),
                        physical,
                        no_gen,
                        visibility: self.member_visibility.clone(),
                    })
                })
                .collect(),
        )
    }

    /// `name(params): return_type is [also|first|only] { statements };`
    fn parse_method(&mut self) -> Option<Member> {
        let start = self.current_span().0;
        // Optional explicit `method` keyword (`method name() is { .. }`).
        self.eat_keyword("method");
        let (name, name_span) = self.expect_word("E0102", "expected the method name")?;
        self.expect_punct(
            "(",
            "E0109",
            "expected '(' to open the method parameter list",
        )?;
        let mut params = Vec::new();
        if !self.at_punct(")") {
            loop {
                let (param_name, param_span) =
                    self.expect_word("E0102", "expected parameter name")?;
                // `name: type` or the untyped `name` (clause 17), which
                // accepts anything.
                let ty = if self.eat_punct(":") {
                    Some(self.parse_type()?)
                } else {
                    None
                };
                params.push(Param {
                    name: param_name,
                    name_span: param_span,
                    ty,
                });
                if !self.eat_punct(",") {
                    break;
                }
            }
        }
        self.expect_punct(
            ")",
            "E0110",
            "expected ')' to close the method parameter list",
        )?;
        let return_type = if self.eat_punct(":") {
            Some(self.parse_type()?)
        } else {
            None
        };
        // TCM suffix: `name() @event [also|first|only] is { .. }` — the
        // event expression is stored as written (e.g. `@sys.any`,
        // `@driver.clock`).
        let tcm_event = if self.at_punct("@") {
            self.bump();
            Some(self.parse_expr()?)
        } else {
            None
        };
        if !self.eat_keyword("is") {
            self.error_here("E0115", "expected 'is' to start the method body");
            self.recover_to_member_boundary();
            return None;
        }
        let modifier = if self.eat_keyword("also") {
            Some(MethodModifier::Also)
        } else if self.eat_keyword("first") {
            Some(MethodModifier::First)
        } else if self.eat_keyword("only") {
            Some(MethodModifier::Only)
        } else {
            None
        };
        // `is empty;` — a body-less method (no-op by declaration).
        // `is undefined;` — declared unimplemented; the same body-less
        // shape (calling it is a later-milestone runtime distinction).
        if self.eat_keyword("empty") || self.eat_keyword("undefined") {
            self.expect_punct(";", "E0105", "expected ';' after the method declaration")?;
            let end = self.last_end();
            return Some(Member::Method(MethodDecl {
                name,
                name_span,
                params,
                return_type,
                tcm_event,
                modifier,
                body: Vec::new(),
                span: (start, end),
            }));
        }
        self.expect_punct("{", "E0106", "expected '{' to open the method body")?;
        let body = self.parse_statements();
        self.expect_punct("}", "E0107", "expected '}' to close the method body")?;
        // The `;` may be omitted when the method is the last member of a
        // body (Specman accepts both spellings).
        if !self.eat_punct(";") && !self.at_punct("}") {
            self.expect_punct(";", "E0105", "expected ';' after the method declaration")?;
        }
        let end = self.last_end();
        Some(Member::Method(MethodDecl {
            name,
            name_span,
            params,
            return_type,
            tcm_event,
            modifier,
            body,
            span: (start, end),
        }))
    }

    /// `name: <dir> simple_port of <type> is instance;` — the direction
    /// spellings are identifiers in the pinned reference grammar, so the
    /// declaration is detected positionally.
    fn peek_is_port_declaration(&self) -> bool {
        let Some(TokenKind::Punctuation(colon)) = self.peek_second_kind() else {
            return false;
        };
        if colon != ":" {
            return false;
        }
        let Some(Token {
            kind: direction_kind,
            ..
        }) = self.tokens.get(self.pos + 2)
        else {
            return false;
        };
        // `in` is a keyword (`for each in …`), `out`/`inout` lex as
        // identifiers — the direction is matched by word text either way.
        let direction = match direction_kind {
            TokenKind::Keyword(word) | TokenKind::Identifier(word) => word.as_str(),
            _ => return false,
        };
        matches!(direction, "in" | "out" | "inout")
            && matches!(
                self.tokens.get(self.pos + 3).map(|t| &t.kind),
                Some(TokenKind::Identifier(id)) if id == "simple_port"
            )
    }

    fn parse_port(&mut self) -> Option<Member> {
        use crate::ast::{PortDecl, PortDirection};
        let start = self.current_span().0;
        let (name, name_span) = self.take_identifier();
        self.expect_punct(":", "E0103", "expected ':' after the port name")?;
        let direction = match self.peek_kind() {
            Some(TokenKind::Keyword(id)) | Some(TokenKind::Identifier(id)) => match id.as_str() {
                "in" => {
                    self.bump();
                    PortDirection::In
                }
                "out" => {
                    self.bump();
                    PortDirection::Out
                }
                "inout" => {
                    self.bump();
                    PortDirection::Inout
                }
                other => {
                    let message =
                        format!("unknown port direction '{other}' (expected in, out or inout)");
                    self.error_here("E0104", &message);
                    return None;
                }
            },
            _ => {
                self.error_here("E0104", "expected a port direction (in, out or inout)");
                return None;
            }
        };
        match self.peek_kind() {
            Some(TokenKind::Identifier(id)) if id == "simple_port" => {
                self.bump();
            }
            _ => {
                self.error_here("E0104", "expected 'simple_port'");
                return None;
            }
        }
        if !self.eat_keyword("of") {
            self.error_here("E0112", "expected 'of' after 'simple_port'");
            return None;
        }
        let element = self.parse_type()?;
        // The `is instance` suffix is syntactic in this subset: every
        // simple_port member is treated as an instance.
        if self.eat_keyword("is") {
            self.eat_keyword("instance");
        }
        self.expect_punct(";", "E0105", "expected ';' after the port declaration")?;
        let end = self.last_end();
        Some(Member::Port(PortDecl {
            name,
            name_span,
            direction,
            element,
            span: (start, end),
        }))
    }

    /// `event name;` or `event name is <trigger>;` — the trigger grammar
    /// covers `@path` attachments (dotted, `or`-composed, with `only`/
    /// `unexp` and `cycle` forms) and `rise(cond) @path`-style edge
    /// samples; edge scheduling itself is a later milestone.
    fn parse_event(&mut self) -> Option<Member> {
        let start = self.current_span().0;
        self.bump(); // 'event'
        let (name, name_span) = self.expect_identifier("E0102", "expected event name")?;
        let trigger = if self.eat_keyword("is") {
            Some(self.parse_event_trigger()?)
        } else {
            None
        };
        self.expect_punct(";", "E0105", "expected ';' after the event declaration")?;
        let end = self.last_end();
        Some(Member::Event(crate::ast::EventDecl {
            name,
            name_span,
            trigger,
            span: (start, end),
        }))
    }

    /// `[only|unexp] [cycle] item (`or` item)*` where an item is
    /// `@dotted.path` or `routine(cond) @dotted.path`.
    fn parse_event_trigger(&mut self) -> Option<crate::ast::EventTrigger> {
        let modifier = if self.eat_word("only") {
            Some("only".to_owned())
        } else if self.eat_word("unexp") {
            Some("unexp".to_owned())
        } else {
            None
        };
        // `cycle` is a keyword only in this position; `event x is cycle`
        // without a following `@` is not a valid trigger, so the lookahead
        // keeps `cycle` usable as an identifier elsewhere.
        let cycle = self.peek_word_is("cycle")
            && matches!(self.peek_second_kind(), Some(TokenKind::Punctuation(p)) if p == "@");
        if cycle {
            self.bump();
        }
        // Grouped guard form: `is (fall(a) and true(b)) @clk` — the
        // sampled path comes after the group and applies to every inner
        // sample.
        if self.eat_punct("(") {
            let mut items = Vec::new();
            let mut group_op = None;
            loop {
                items.push(self.parse_edge_sample()?);
                if self.eat_keyword("and") {
                    group_op = Some("and".to_owned());
                } else if self.eat_keyword("or") {
                    group_op = Some("or".to_owned());
                } else {
                    break;
                }
            }
            self.expect_punct(")", "E0110", "expected ')' to close the trigger group")?;
            self.expect_punct("@", "E0108", "expected '@' in the event trigger")?;
            let path = self.parse_dotted_path()?;
            for item in &mut items {
                if item.path.is_empty() {
                    item.path = path.clone();
                }
            }
            return Some(crate::ast::EventTrigger {
                modifier,
                cycle,
                items,
                group_op,
            });
        }
        let mut items = Vec::new();
        loop {
            let item = self.parse_edge_sample_with_path()?;
            items.push(item);
            if !self.eat_keyword("or") {
                break;
            }
        }
        Some(crate::ast::EventTrigger {
            modifier,
            cycle,
            items,
            group_op: None,
        })
    }

    /// One `@path` or `routine(cond) @path` sample (grouped-guard form;
    /// the `@path` is filled in by the caller).
    fn parse_edge_sample(&mut self) -> Option<crate::ast::EventTriggerItem> {
        let (routine, condition) = if self.peek_is_edge_routine() {
            let routine = self.take_word()?;
            if !matches!(routine.as_str(), "rise" | "fall" | "change" | "true") {
                self.error_here("E0102", &format!("unknown edge routine '{routine}'"));
                return None;
            }
            self.expect_punct("(", "E0108", "expected '(' after the edge routine")?;
            let condition = self.parse_expr()?;
            self.expect_punct(")", "E0108", "expected ')' after the edge condition")?;
            (Some(routine), Some(condition))
        } else {
            (None, None)
        };
        Some(crate::ast::EventTriggerItem {
            routine,
            path: Vec::new(),
            condition,
        })
    }

    /// One `@path` or `routine(cond) @path` sample of an ungrouped
    /// trigger.
    fn parse_edge_sample_with_path(&mut self) -> Option<crate::ast::EventTriggerItem> {
        let item = self.parse_edge_sample()?;
        self.expect_punct("@", "E0108", "expected '@' in the event trigger")?;
        let path = self.parse_dotted_path()?;
        Some(crate::ast::EventTriggerItem {
            routine: item.routine,
            path,
            condition: item.condition,
        })
    }

    /// A dotted identifier path whose segments may end in the HDL `$`
    /// suffix (`p_smp.clk`, `off_p$`).
    fn parse_dotted_path(&mut self) -> Option<Vec<String>> {
        let mut segments = Vec::new();
        loop {
            let (segment, _) =
                self.expect_identifier("E0102", "expected a path segment in the trigger")?;
            segments.push(segment);
            if !self.eat_punct(".") {
                break;
            }
        }
        Some(segments)
    }

    /// Consume tokens up to (not including) the terminating `;` and
    /// return their concatenated text as an HDL path. Used for `keep
    /// bind … to <hdl_path>` targets, which are simulator-side paths
    /// rather than e expressions.
    fn parse_raw_path_tokens(&mut self) -> Expr {
        let start = self.current_span().0;
        let mut path = String::new();
        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Punctuation(p) if p == ";" => break,
                TokenKind::Identifier(text)
                | TokenKind::Keyword(text)
                | TokenKind::Number(text)
                | TokenKind::Punctuation(text) => path.push_str(text),
                TokenKind::StringLiteral(text) => {
                    path.push('"');
                    path.push_str(text);
                    path.push('"');
                }
            }
            self.bump();
        }
        Expr::HdlPath {
            path,
            span: (start, self.last_end()),
        }
    }

    /// The postfix loop shared by every primary that can carry one:
    /// `.member` access, `.as_a(type)` casts, calls and indexing. Member
    /// names accept keyword spellings (`as_a` is a keyword; predefined
    /// member names such as `kind` may collide with other keywords).
    fn parse_postfix_chain(&mut self, mut expr: Expr, start: Position) -> Option<Expr> {
        loop {
            if self.at_punct(".") {
                self.bump();
                let (member, member_span) = if self.at_identifier() {
                    self.take_identifier()
                } else if let Some(word) = self.take_word() {
                    let span = self.last_end();
                    (word, (span, span))
                } else {
                    self.error_here("E0102", "expected member name after '.'");
                    return None;
                };
                // `expr.as_a(type)` cast.
                if member == "as_a" && self.at_punct("(") {
                    self.bump();
                    let ty = self.parse_type()?;
                    self.expect_punct(")", "E0110", "expected ')' to close the cast")?;
                    expr = Expr::CastAs {
                        expr: Box::new(expr),
                        ty,
                        span: (start, self.last_end()),
                    };
                    continue;
                }
                expr = Expr::MemberAccess {
                    object: Box::new(expr),
                    name: member,
                    name_span: member_span,
                    span: (start, member_span.1),
                };
            } else if self.at_punct("(") {
                self.bump();
                let mut args = Vec::new();
                if !self.at_punct(")") {
                    loop {
                        args.push(self.parse_expr()?);
                        if !self.eat_punct(",") {
                            break;
                        }
                    }
                }
                self.expect_punct(")", "E0110", "expected ')' to close the call arguments")?;
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                    span: (start, self.last_end()),
                };
            } else if self.at_punct("[") {
                self.bump();
                let first = self.parse_expr()?;
                if self.eat_punct(":") {
                    // Bit slice: base[high:low].
                    let low = self.parse_expr()?;
                    self.expect_punct("]", "E0107", "expected ']' to close the bit slice")?;
                    expr = Expr::BitSlice {
                        base: Box::new(expr),
                        high: Box::new(first),
                        low: Box::new(low),
                        span: (start, self.last_end()),
                    };
                } else {
                    self.expect_punct("]", "E0107", "expected ']' to close the index expression")?;
                    expr = Expr::Index {
                        base: Box::new(expr),
                        index: Box::new(first),
                        span: (start, self.last_end()),
                    };
                }
            } else {
                break;
            }
        }
        Some(expr)
    }

    /// The keyword or identifier text at the cursor, if any.
    fn peek_word(&self) -> Option<&str> {
        match self.peek_kind() {
            Some(TokenKind::Keyword(k)) => Some(k.as_str()),
            Some(TokenKind::Identifier(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    fn peek_word_is(&self, word: &str) -> bool {
        self.peek_word() == Some(word)
    }

    fn eat_word(&mut self, word: &str) -> bool {
        if self.peek_word_is(word) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Consume and return the keyword or identifier text at the cursor.
    fn take_word(&mut self) -> Option<String> {
        match self.peek_kind() {
            Some(TokenKind::Keyword(k)) => {
                let text = k.clone();
                self.bump();
                Some(text)
            }
            Some(TokenKind::Identifier(s)) => {
                let text = s.clone();
                self.bump();
                Some(text)
            }
            _ => None,
        }
    }

    /// `rise`/`fall`/`change`/`true` immediately followed by `(` — the
    /// distinguishing lookahead against an `@path` item.
    fn peek_is_edge_routine(&self) -> bool {
        matches!(
            self.peek_word(),
            Some("rise") | Some("fall") | Some("change") | Some("true")
        ) && matches!(
            self.peek_second_kind(),
            Some(TokenKind::Punctuation(p)) if p == "("
        )
    }

    fn parse_keep(&mut self) -> Option<KeepConstraint> {
        let start = self.current_span().0;
        self.bump(); // 'keep'
        let is_soft = self.eat_keyword("soft");
        // `keep bind (port, hdl_kind);` — hierarchical HDL binding,
        // recorded as a `bind(a, b)` call and skipped by checking/gen.
        // The template form inside `define … as` bodies is macro
        // machinery and still fails here.
        let is_bind = self.eat_keyword("bind");
        let expr = if is_bind {
            // Both corpus shapes: `keep bind (port, kind);` and the UVM-e
            // `keep bind bfm to hdl_instance;`.
            let first = if self.eat_punct("(") {
                let first = self.parse_expr()?;
                self.expect_punct(",", "E0108", "expected ',' between the bind operands")?;
                let second = self.parse_expr()?;
                self.expect_punct(")", "E0108", "expected ')' after the bind operands")?;
                vec![first, second]
            } else {
                let first = self.parse_expr()?;
                self.expect_keyword_word("to")?;
                // The bind target is an HDL instance path (`~/dut/u0`);
                // store it as written rather than forcing it into the e
                // expression grammar.
                let second = self.parse_raw_path_tokens();
                vec![first, second]
            };
            Expr::Call {
                callee: Box::new(Expr::Ident {
                    name: "bind".to_owned(),
                    span: (start, start),
                }),
                args: first,
                span: (start, self.last_end()),
            }
        } else {
            self.parse_expr()?
        };
        self.expect_punct(";", "E0105", "expected ';' after keep constraint")?;
        let end = self.last_end();
        Some(KeepConstraint {
            expr,
            is_soft,
            is_bind,
            span: (start, end),
        })
    }

    fn parse_type(&mut self) -> Option<TypeExpr> {
        let (start, _) = self.current_span();
        if self.at_punct("[") {
            // Inline enumeral list type: [imm, reg].
            self.bump();
            let mut names = Vec::new();
            loop {
                let (name, _) = self.expect_identifier("E0102", "expected an enumeral name")?;
                names.push(name);
                if !self.eat_punct(",") {
                    break;
                }
            }
            self.expect_punct("]", "E0107", "expected ']' to close the enumeral list")?;
            let end = self.last_end();
            return Some(TypeExpr::InlineEnum {
                names,
                span: (start, end),
            });
        }
        if self.at_punct("<") {
            // A template's formal parameter in a type position
            // (`<type>`, `<key'type>` — clause 8.1): recorded as the raw
            // text between the brackets.
            self.bump();
            let (name, _) = self.expect_word("E0102", "expected the formal parameter name")?;
            let mut name = name;
            if self.eat_punct("'") {
                let (category, _) = self.expect_word("E0102", "expected the parameter category")?;
                name.push('\'');
                name.push_str(&category);
            }
            self.expect_punct(">", "E0107", "expected '>' to close the formal parameter")?;
            let end = self.last_end();
            return Some(TypeExpr::Formal {
                name,
                span: (start, end),
            });
        }
        if self.at_identifier() {
            let (name, span) = self.take_identifier();
            // A when-qualified type (`reg cpu_instr_s`, the modifier
            // naming the subtype variant) — the subtype qualifier is
            // dropped; subtype dispatch is not modeled yet.
            let (name, span) = if self.at_identifier() {
                let (base, base_span) = self.take_identifier();
                (base, base_span)
            } else {
                (name, span)
            };
            // `deque of int` — a template instantiation in a type
            // position (clause 8.3). `list of` never reaches here: the
            // keyword branch handles it first.
            if self.at_keyword("of") {
                self.bump();
                let args = if self.eat_punct("(") {
                    let mut args = vec![self.parse_type()?];
                    while self.eat_punct(",") {
                        args.push(self.parse_type()?);
                    }
                    self.expect_punct(
                        ")",
                        "E0110",
                        "expected ')' to close the template arguments",
                    )?;
                    args
                } else {
                    vec![self.parse_type()?]
                };
                let end = self.last_end();
                return Some(TypeExpr::TemplateRef {
                    name,
                    args,
                    span: (start, end),
                });
            }
            return Some(TypeExpr::Named { name, span });
        }
        let Some(TokenKind::Keyword(keyword)) = self.peek_kind() else {
            self.error_here("E0104", "expected a type");
            return None;
        };
        let keyword = keyword.clone();
        if keyword == "list" {
            self.bump();
            // Keyed list: `list (key:it) of <type>` — the key expression
            // is recorded (clause 5.1.8's keyed lists).
            let key = if self.at_punct("(") && self.peek_second_is_word("key") {
                self.bump();
                self.expect_keyword_word("key")?;
                self.expect_punct(":", "E0103", "expected ':' after 'key'")?;
                let key = self.parse_expr()?;
                self.expect_punct(")", "E0110", "expected ')' after the key specification")?;
                Some(Box::new(key))
            } else {
                None
            };
            if !self.eat_keyword("of") {
                self.error_here("E0112", "expected 'of' after 'list'");
                return None;
            }
            let element = Box::new(self.parse_type()?);
            let end = element.span().1;
            return Some(TypeExpr::List {
                element,
                key,
                span: (start, end),
            });
        }
        if let Some(base) = ScalarBase::from_keyword(&keyword) {
            self.bump();
            let mut width = None;
            if (base == ScalarBase::Uint || base == ScalarBase::Int) && self.at_punct("(") {
                self.bump();
                if !self.eat_keyword("bits") {
                    self.error_here("E0111", "expected 'bits' in width specification");
                    return None;
                }
                self.expect_punct(":", "E0103", "expected ':' after 'bits'")?;
                let expr = self.parse_expr()?;
                width = Some(Box::new(expr));
                self.expect_punct(")", "E0110", "expected ')' after width specification")?;
            }
            let end = self.last_end();
            return Some(TypeExpr::Scalar {
                base,
                width,
                span: (start, end),
            });
        }
        if keyword == "file" || keyword == "event" {
            // Predefined non-scalar types (`var f: file`, `ev: event;`);
            // recorded as named types and accepted by the
            // well-formedness check.
            self.bump();
            let end = self.last_end();
            return Some(TypeExpr::Named {
                name: keyword,
                span: (start, end),
            });
        }
        self.error_here("E0104", "expected a type");
        None
    }

    // ---- statements ----

    /// Statements up to (not including) the closing `}` of the enclosing
    /// block.
    fn parse_statements(&mut self) -> Vec<Stmt> {
        let mut statements = Vec::new();
        while self.peek().is_some() && !self.at_punct("}") {
            // `#` directives are actions too (clause 21.2); an excluded
            // branch contributes no statements and the loop continues.
            if self.at_punct("#") {
                if let Some(hash_statements) = self.parse_hash_statements() {
                    statements.extend(hash_statements);
                }
                continue;
            }
            if let Some(statement) = self.parse_statement() {
                statements.push(statement);
            } else {
                self.recover_to_member_boundary();
            }
        }
        statements
    }

    fn parse_statement(&mut self) -> Option<Stmt> {
        let start = self.current_span().0;
        match self.peek_kind() {
            Some(TokenKind::Keyword(k)) if k == "var" => {
                self.bump();
                let (name, name_span) = self.expect_word("E0102", "expected variable name")?;
                // `var x: t = e;` or the type-inferring `var x := e;`
                // (clause 18) — the inferred type is resolved at run
                // time from the initializer.
                let (ty, init) = if self.eat_punct(":=") {
                    (None, Some(self.parse_expr()?))
                } else {
                    self.expect_punct(":", "E0103", "expected ':' after the variable name")?;
                    let ty = Some(self.parse_type()?);
                    let init = if self.eat_punct("=") {
                        Some(self.parse_expr()?)
                    } else {
                        None
                    };
                    (ty, init)
                };
                self.expect_punct(";", "E0105", "expected ';' after variable declaration")?;
                let end = self.last_end();
                Some(Stmt::VarDecl {
                    name,
                    name_span,
                    ty,
                    init,
                    span: (start, end),
                })
            }
            Some(TokenKind::Keyword(k)) if k == "if" => self.parse_if_statement(),
            Some(TokenKind::Keyword(k)) if k == "while" => self.parse_while_statement(),
            Some(TokenKind::Keyword(k)) if k == "break" => {
                self.bump();
                self.expect_punct(";", "E0105", "expected ';' after 'break'")?;
                let end = self.last_end();
                Some(Stmt::Break { span: (start, end) })
            }
            Some(TokenKind::Keyword(k)) if k == "continue" => {
                self.bump();
                self.expect_punct(";", "E0105", "expected ';' after 'continue'")?;
                let end = self.last_end();
                Some(Stmt::Continue { span: (start, end) })
            }
            Some(TokenKind::Keyword(k)) if k == "return" => {
                self.bump();
                let value = if self.at_punct(";") {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                self.expect_stmt_semi("expected ';' after 'return'")?;
                let end = self.last_end();
                Some(Stmt::Return {
                    value,
                    span: (start, end),
                })
            }
            Some(TokenKind::Keyword(k)) if k == "print" => {
                self.bump();
                let mut exprs = Vec::new();
                if !self.at_punct(";") {
                    loop {
                        exprs.push(self.parse_expr()?);
                        if !self.eat_punct(",") {
                            break;
                        }
                    }
                }
                self.expect_punct(";", "E0105", "expected ';' after 'print'")?;
                let end = self.last_end();
                Some(Stmt::Print {
                    exprs,
                    span: (start, end),
                })
            }
            Some(TokenKind::Keyword(k)) if k == "gen" => {
                self.bump();
                let expr = self.parse_expr()?;
                self.expect_stmt_semi("expected ';' after 'gen'")?;
                let end = self.last_end();
                Some(Stmt::Gen {
                    expr,
                    span: (start, end),
                })
            }
            Some(TokenKind::Keyword(k)) if k == "case" => self.parse_case_statement(),
            Some(TokenKind::Keyword(k)) if k == "wait" => self.parse_wait_statement(),
            Some(TokenKind::Keyword(k)) if k == "for" => self.parse_for_statement(),
            Some(TokenKind::Keyword(k)) if k == "emit" => {
                self.bump();
                let target = self.parse_expr()?;
                self.expect_stmt_semi("expected ';' after 'emit'")?;
                let end = self.last_end();
                Some(Stmt::Emit {
                    target,
                    span: (start, end),
                })
            }
            Some(TokenKind::Keyword(k)) if k == "start" => {
                self.bump();
                let call = self.parse_expr()?;
                self.expect_stmt_semi("expected ';' after 'start'")?;
                let end = self.last_end();
                Some(Stmt::Start {
                    call,
                    span: (start, end),
                })
            }
            // `compute <call>;` (clause 13) — parsed like `start`;
            // without TCM suspension the two execute the same way.
            Some(TokenKind::Keyword(k)) if k == "compute" => {
                self.bump();
                let call = self.parse_expr()?;
                self.expect_stmt_semi("expected ';' after 'compute'")?;
                let end = self.last_end();
                Some(Stmt::Compute {
                    call,
                    span: (start, end),
                })
            }
            Some(TokenKind::Keyword(k)) if k == "check" => {
                // `check that <cond> else <stmt-or-block>;` — the `that`
                // keyword is optional in the reference grammar.
                self.bump();
                self.eat_keyword("that");
                let condition = self.parse_expr()?;
                let else_body = if self.eat_keyword("else") {
                    if self.eat_punct("{") {
                        let body = self.parse_statements();
                        self.expect_punct("}", "E0107", "expected '}' to close the check body")?;
                        self.eat_punct(";");
                        body
                    } else {
                        // A single-statement else carries its own `;`.
                        let statement = self.parse_statement()?;
                        vec![statement]
                    }
                } else {
                    self.eat_punct(";");
                    Vec::new()
                };
                let end = self.last_end();
                Some(Stmt::Check {
                    condition,
                    else_body,
                    span: (start, end),
                })
            }
            Some(TokenKind::Keyword(k)) if k == "do" => {
                // `do <item-field>;` — a sequence body item execution.
                self.bump();
                let (item, _) =
                    self.expect_identifier("E0102", "expected the item field after 'do'")?;
                // The `do trans keeping { … }` form: the boolean block
                // constrains the generated item; recorded, not enforced
                // (constraint-scoped gen is a later milestone).
                if self.eat_keyword("keeping") {
                    self.expect_punct("{", "E0106", "expected '{' to open the keeping block")?;
                    self.parse_expr()?;
                    self.expect_punct("}", "E0107", "expected '}' to close the keeping block")?;
                }
                self.expect_punct(";", "E0105", "expected ';' after the do statement")?;
                let end = self.last_end();
                Some(Stmt::SeqDo {
                    item,
                    span: (start, end),
                })
            }
            Some(TokenKind::Keyword(k)) if is_predefined_name(k) => {
                // A statement whose target is a predefined name (`it.x =
                // p_out$;`) — parse as the expression/assignment form.
                let expr = self.parse_expr()?;
                if self.eat_punct("=") {
                    let value = self.parse_expr()?;
                    // The last statement of a block may omit the `;`.
                    if !self.eat_punct(";") && !self.at_punct("}") {
                        self.expect_punct(";", "E0105", "expected ';' after assignment")?;
                    }
                    let end = self.last_end();
                    Some(Stmt::Assign {
                        target: expr,
                        value,
                        span: (start, end),
                    })
                } else if !self.eat_punct(";") && !self.at_punct("}") {
                    self.expect_punct(";", "E0105", "expected ';' after statement")?;
                    let end = self.last_end();
                    Some(Stmt::ExprStmt {
                        expr,
                        span: (start, end),
                    })
                } else {
                    let end = self.last_end();
                    Some(Stmt::ExprStmt {
                        expr,
                        span: (start, end),
                    })
                }
            }
            Some(TokenKind::Keyword(_)) => {
                self.error_here("E0113", "this statement kind is not supported yet");
                self.recover_to_member_boundary();
                None
            }
            _ => {
                let expr = self.parse_expr()?;
                if self.eat_punct("=") {
                    let value = self.parse_expr()?;
                    // The last statement of a block may omit the `;`.
                    if !self.eat_punct(";") && !self.at_punct("}") {
                        self.expect_punct(";", "E0105", "expected ';' after assignment")?;
                    }
                    let end = self.last_end();
                    Some(Stmt::Assign {
                        target: expr,
                        value,
                        span: (start, end),
                    })
                } else if self.at_punct("+=") || self.at_punct("-=") {
                    // `x += n` desugars to `x = x + n` (and `-=` likewise);
                    // no other compound operators occur in the corpora.
                    let TokenKind::Punctuation(op) = self.bump().kind else {
                        unreachable!("peeked a compound assignment operator");
                    };
                    let operand = self.parse_expr()?;
                    let value = Expr::Binary {
                        op: if op == "+=" {
                            crate::ast::BinaryOp::Add
                        } else {
                            crate::ast::BinaryOp::Sub
                        },
                        lhs: Box::new(expr.clone()),
                        rhs: Box::new(operand),
                        span: (start, self.last_end()),
                    };
                    self.expect_punct(";", "E0105", "expected ';' after assignment")?;
                    let end = self.last_end();
                    Some(Stmt::Assign {
                        target: expr,
                        value,
                        span: (start, end),
                    })
                } else {
                    // The final statement of a `{ .. }` body may omit the
                    // `;` (Specman accepts both spellings).
                    if !self.eat_punct(";") && !self.at_punct("}") {
                        self.expect_punct(";", "E0105", "expected ';' after statement")?;
                    }
                    let end = self.last_end();
                    Some(Stmt::ExprStmt {
                        expr,
                        span: (start, end),
                    })
                }
            }
        }
    }

    /// `if cond { statements } else { statements };` — the braced form only;
    /// `if ... do ..` and loops are a later milestone.
    fn parse_if_statement(&mut self) -> Option<Stmt> {
        let inner = self.parse_if_inner()?;
        self.expect_stmt_semi("expected ';' after the 'if' statement")?;
        Some(inner)
    }

    /// The `if`/`else if` shape without its statement terminator —
    /// `else if` chains share the outer statement's `;`.
    fn parse_if_inner(&mut self) -> Option<Stmt> {
        let start = self.current_span().0;
        self.bump(); // 'if'
        let cond = self.parse_expr()?;
        // Both `if cond { .. }` and `if cond then { .. }` are accepted.
        self.eat_keyword("then");
        if self
            .expect_punct("{", "E0106", "expected '{' to open the 'if' body")
            .is_none()
        {
            self.recover_to_member_boundary();
            return None;
        }
        let then_body = self.parse_statements();
        self.expect_punct("}", "E0107", "expected '}' to close the 'if' body")?;
        let else_body = if self.eat_keyword("else") {
            if self.at_keyword("if") {
                // `else if` chains nest as a one-statement else body.
                let inner = self.parse_if_inner()?;
                Some(vec![inner])
            } else {
                if self
                    .expect_punct("{", "E0106", "expected '{' to open the 'else' body")
                    .is_none()
                {
                    self.recover_to_member_boundary();
                    return None;
                }
                let body = self.parse_statements();
                self.expect_punct("}", "E0107", "expected '}' to close the 'else' body")?;
                Some(body)
            }
        } else {
            None
        };
        let end = self.last_end();
        Some(Stmt::If {
            cond,
            then_body,
            else_body,
            span: (start, end),
        })
    }

    /// `while cond { statements };` — other loop forms are a later
    /// milestone.
    fn parse_while_statement(&mut self) -> Option<Stmt> {
        let start = self.current_span().0;
        self.bump(); // 'while'
        let cond = self.parse_expr()?;
        if self
            .expect_punct("{", "E0106", "expected '{' to open the 'while' body")
            .is_none()
        {
            self.recover_to_member_boundary();
            return None;
        }
        let body = self.parse_statements();
        self.expect_punct("}", "E0107", "expected '}' to close the 'while' body")?;
        self.expect_stmt_semi("expected ';' after the 'while' statement")?;
        let end = self.last_end();
        Some(Stmt::While {
            cond,
            body,
            span: (start, end),
        })
    }

    /// `case expr { alt: {..}; [a, b]: {..}; default: {..}; };` — the
    /// statement form; alternatives may be grouped with `[a, b]` or
    /// chained with `|`, and the trailing `;` is accepted but not
    /// required after arm bodies.
    fn parse_case_statement(&mut self) -> Option<Stmt> {
        let start = self.current_span().0;
        self.bump(); // 'case'
        let scrutinee = self.parse_expr()?;
        self.expect_punct("{", "E0106", "expected '{' to open the 'case' body")?;
        let mut arms: Vec<(Vec<Expr>, Vec<Stmt>)> = Vec::new();
        let mut default: Option<Vec<Stmt>> = None;
        while self.peek().is_some() && !self.at_punct("}") {
            let mut alternatives = Vec::new();
            if self.eat_keyword("default") {
                // the default arm: empty alternatives marks it
            } else {
                loop {
                    if self.eat_punct("[") {
                        loop {
                            alternatives.push(self.parse_expr()?);
                            if !self.eat_punct(",") {
                                break;
                            }
                        }
                        self.expect_punct(
                            "]",
                            "E0107",
                            "expected ']' to close the alternative group",
                        )?;
                    } else {
                        alternatives.push(self.parse_expr()?);
                    }
                    if !self.eat_punct("|") {
                        break;
                    }
                }
            }
            self.expect_punct(":", "E0103", "expected ':' after the 'case' alternative")?;
            self.expect_punct("{", "E0106", "expected '{' to open the 'case' arm")?;
            let body = self.parse_statements();
            self.expect_punct("}", "E0107", "expected '}' to close the 'case' arm")?;
            self.eat_punct(";");
            if alternatives.is_empty() {
                default = Some(body);
            } else {
                arms.push((alternatives, body));
            }
        }
        self.expect_punct("}", "E0107", "expected '}' to close the 'case' body")?;
        self.eat_punct(";");
        let end = self.last_end();
        Some(Stmt::Case {
            scrutinee,
            arms,
            default,
            span: (start, end),
        })
    }

    /// `wait cycle;` / `wait [n];` / `wait [n] * cycle;` — normalized to
    /// a cycle-count expression. In this v1 kernel time units and cycles
    /// advance the same counter (documented in the interpreter).
    fn parse_wait_statement(&mut self) -> Option<Stmt> {
        let start = self.current_span().0;
        self.bump(); // 'wait'
        let cycles = if self.eat_keyword("cycle") {
            Expr::IntLiteral {
                text: "1".to_owned(),
                span: (start, self.last_end()),
            }
        } else if self.eat_keyword("until") {
            // `wait until <expr>` samples a temporal condition; edge
            // sampling is a later milestone, so the condition is checked
            // for syntax and the wait advances one cycle for now.
            self.parse_expr()?;
            Expr::IntLiteral {
                text: "1".to_owned(),
                span: (start, self.last_end()),
            }
        } else {
            self.expect_punct("[", "E0106", "expected 'cycle' or '[n]' after 'wait'")?;
            let count = self.parse_expr()?;
            self.expect_punct("]", "E0107", "expected ']' to close the wait count")?;
            if self.eat_punct("*") && !self.eat_keyword("cycle") {
                self.error_here("E0113", "expected 'cycle' after the wait multiplier");
                return None;
            }
            count
        };
        self.expect_stmt_semi("expected ';' after 'wait'")?;
        let end = self.last_end();
        Some(Stmt::Wait {
            cycles,
            span: (start, end),
        })
    }

    /// `for each in <list> do { .. };` and
    /// `for <var> from <a> to <b> { .. };`
    fn parse_for_statement(&mut self) -> Option<Stmt> {
        let start = self.current_span().0;
        self.bump(); // 'for'
        if self.eat_keyword("each") {
            // Optional typed binding: `for each rf_struct (rs) in list` —
            // the element type filter is recorded, not enforced.
            let mut type_filter = None;
            if self.at_identifier()
                && matches!(self.peek_second_kind(), Some(TokenKind::Punctuation(p)) if p == "(")
            {
                let (type_name, _) = self.take_identifier();
                type_filter = Some(type_name);
            }
            // Optional named binding: `for each (m) in list do`.
            let var = if self.eat_punct("(") {
                let (name, _) =
                    self.expect_word("E0102", "expected the iterator name after 'for each ('")?;
                self.expect_punct(")", "E0110", "expected ')' after the iterator name")?;
                Some(name)
            } else {
                None
            };
            // `using <index-name> (idx)` — the index binding form; the
            // name is a (dotted) field path, recorded. A full expression
            // parse would swallow `(idx)` as a call.
            let mut using = None;
            let mut index_var = None;
            if self.eat_keyword("using") {
                let (first, first_span) =
                    self.expect_word("E0102", "expected the index field after 'using'")?;
                let mut path = Expr::Ident {
                    name: first,
                    span: first_span,
                };
                while self.at_punct(".") {
                    self.bump();
                    let (segment, segment_span) =
                        self.expect_word("E0102", "expected a path segment after '.'")?;
                    let end = self.last_end();
                    path = Expr::MemberAccess {
                        object: Box::new(path),
                        name: segment,
                        name_span: segment_span,
                        span: (first_span.0, end),
                    };
                }
                using = Some(path);
                self.expect_punct("(", "E0106", "expected '(' before the index name")?;
                let (name, _) =
                    self.expect_word("E0102", "expected the index name after 'using'")?;
                self.expect_punct(")", "E0110", "expected ')' after the index name")?;
                index_var = Some(name);
            }
            self.expect_keyword_word("in")?;
            // `for each in reverse my_list` — iteration in reverse order.
            let reversed = self.eat_keyword("reverse");
            let list = self.parse_expr()?;
            self.eat_keyword("do");
            self.expect_punct("{", "E0106", "expected '{' to open the for-each body")?;
            let body = self.parse_statements();
            self.expect_punct("}", "E0107", "expected '}' to close the for-each body")?;
            self.expect_stmt_semi("expected ';' after the for-each statement")?;
            let end = self.last_end();
            return Some(Stmt::ForEach {
                var,
                type_filter,
                index_var,
                using,
                reversed,
                list,
                body,
                span: (start, end),
            });
        }
        let (var, _) = self.expect_identifier("E0102", "expected the loop variable after 'for'")?;
        self.expect_keyword_word("from")?;
        let from = self.parse_expr()?;
        // `down to` counts down (clause 20); plain `to` counts up.
        let down = self.eat_keyword("down");
        self.expect_keyword_word("to")?;
        let to = self.parse_expr()?;
        // `do` before `{` is the spelled form; both spellings occur.
        self.eat_keyword("do");
        self.expect_punct("{", "E0106", "expected '{' to open the for body")?;
        let body = self.parse_statements();
        self.expect_punct("}", "E0107", "expected '}' to close the for body")?;
        self.expect_stmt_semi("expected ';' after the for statement")?;
        let end = self.last_end();
        Some(Stmt::ForTo {
            var,
            from,
            to,
            down,
            body,
            span: (start, end),
        })
    }

    /// Consume a keyword-word (e.g. `in`, `do`, `from`, `to`) or record
    /// a diagnostic and fail.
    fn expect_keyword_word(&mut self, word: &str) -> Option<()> {
        if self.eat_keyword(word) {
            Some(())
        } else {
            self.error_here("E0102", &format!("expected '{word}'"));
            None
        }
    }

    /// The statement terminator: `;` required, except as the last
    /// statement of a block, where the closing `}` may follow directly
    /// (Specman accepts both spellings).
    fn expect_stmt_semi(&mut self, message: &'static str) -> Option<()> {
        if self.eat_punct(";") || self.at_punct("}") {
            return Some(());
        }
        self.expect_punct(";", "E0105", message)
    }

    // ---- expressions, loosest to tightest binding ----

    fn parse_expr(&mut self) -> Option<Expr> {
        let first = self.parse_binary_level(0)?;
        if self.eat_punct("?") {
            let then_expr = self.parse_expr()?;
            self.expect_punct(":", "E0103", "expected ':' in the ternary expression")?;
            let else_expr = self.parse_expr()?;
            let start = first.span().0;
            let end = else_expr.span().1;
            let ternary = Expr::Ternary {
                cond: Box::new(first),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
                span: (start, end),
            };
            return self.finish_binary_level(0, BINARY_LEVELS[0], ternary);
        }
        Some(first)
    }

    fn parse_binary_level(&mut self, level: usize) -> Option<Expr> {
        match level {
            IN_LEVEL => return self.parse_in_level(),
            MATCH_LEVEL => return self.parse_match_level(),
            ISA_LEVEL => return self.parse_isa_level(),
            _ => {}
        }
        let Some(operators) = binary_row_for(level) else {
            return self.parse_unary();
        };
        let lhs = self.parse_binary_level(level + 1)?;
        self.finish_binary_level(level, operators, lhs)
    }

    /// `expr in [a, b]` / `expr in [low .. high]`, left-associative, at
    /// [`IN_LEVEL`].
    fn parse_in_level(&mut self) -> Option<Expr> {
        let mut lhs = self.parse_binary_level(IN_LEVEL + 1)?;
        while self.at_keyword("in") {
            self.bump();
            self.expect_punct("[", "E0106", "expected '[' after 'in'")?;
            let first_item = self.parse_expr()?;
            let set = if self.eat_punct("..") {
                let high = self.parse_expr()?;
                self.expect_punct("]", "E0107", "expected ']' to close the range")?;
                crate::ast::InSetExpr::Range(Box::new(first_item), Box::new(high))
            } else {
                let mut items = vec![first_item];
                while self.eat_punct(",") {
                    items.push(self.parse_expr()?);
                }
                self.expect_punct("]", "E0107", "expected ']' to close the set")?;
                crate::ast::InSetExpr::Items(items)
            };
            let start = lhs.span().0;
            let end = self.last_end();
            lhs = Expr::InSet {
                element: Box::new(lhs),
                set,
                span: (start, end),
            };
        }
        Some(lhs)
    }

    /// `lhs ~ pattern` / `lhs !~ pattern` string match (clause 4.11),
    /// left-associative, at [`MATCH_LEVEL`].
    fn parse_match_level(&mut self) -> Option<Expr> {
        let mut lhs = self.parse_binary_level(MATCH_LEVEL + 1)?;
        loop {
            let op = match self.peek_kind() {
                Some(TokenKind::Punctuation(p)) if p == "~" => BinaryOp::Match,
                Some(TokenKind::Punctuation(p)) if p == "!~" => BinaryOp::NotMatch,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_binary_level(MATCH_LEVEL + 1)?;
            let start = lhs.span().0;
            let end = rhs.span().1;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span: (start, end),
            };
        }
        Some(lhs)
    }

    /// `expr is [not] a <type> [(name)]` type check (clause 4.5),
    /// left-associative, at [`ISA_LEVEL`]. The optional `(name)` binds a
    /// true match to a local. A bare `is` that does not start this shape
    /// is left for the caller — `is` also appears in member and trigger
    /// syntax.
    fn parse_isa_level(&mut self) -> Option<Expr> {
        let mut lhs = self.parse_binary_level(ISA_LEVEL + 1)?;
        while self.peek_is_type_check() {
            self.bump(); // 'is'
            let negated = self.eat_keyword("not");
            self.bump(); // 'a'
            let ty = self.parse_type()?;
            let binding = if self.eat_punct("(") {
                let (name, _) = self.expect_word("E0102", "expected the binding name")?;
                self.expect_punct(")", "E0110", "expected ')' after the binding name")?;
                Some(name)
            } else {
                None
            };
            let start = lhs.span().0;
            let end = self.last_end();
            lhs = Expr::IsA {
                expr: Box::new(lhs),
                ty,
                binding,
                negated,
                span: (start, end),
            };
        }
        Some(lhs)
    }

    /// `is`, optionally `is not`, directly followed by `a` — the shape of
    /// the type check operator. Multi-token lookahead so a different use of
    /// `is` is never consumed here.
    fn peek_is_type_check(&self) -> bool {
        if !self.at_keyword("is") {
            return false;
        }
        match self.peek_second_kind() {
            Some(TokenKind::Identifier(word)) => word == "a",
            Some(TokenKind::Keyword(word)) if word == "not" => {
                matches!(
                    self.tokens.get(self.pos + 2).map(|t| &t.kind),
                    Some(TokenKind::Identifier(word)) if word == "a"
                )
            }
            _ => false,
        }
    }
    /// The left-associative loop for one precedence level, seeded with an
    /// already-parsed left-hand side.
    fn finish_binary_level(
        &mut self,
        level: usize,
        operators: &[(&str, BinaryOp)],
        mut lhs: Expr,
    ) -> Option<Expr> {
        loop {
            let op = match self.peek_kind() {
                Some(TokenKind::Punctuation(symbol)) => operators
                    .iter()
                    .find(|(text, _)| text == symbol)
                    .map(|(_, op)| *op),
                Some(TokenKind::Keyword(word)) => operators
                    .iter()
                    .find(|(text, _)| *text == word.as_str())
                    .map(|(_, op)| *op),
                _ => None,
            };
            let Some(op) = op else { break };
            self.bump();
            let rhs = self.parse_binary_level(level + 1)?;
            let start = lhs.span().0;
            let end = rhs.span().1;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span: (start, end),
            };
        }
        Some(lhs)
    }

    fn parse_unary(&mut self) -> Option<Expr> {
        let (start, _) = self.current_span();
        // `not` is the word-spelled logical negation.
        if self.eat_keyword("not") {
            let inner = self.parse_unary()?;
            let end = inner.span().1;
            return Some(Expr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(inner),
                span: (start, end),
            });
        }
        // A mid-expression `soft` prefix (`a => soft b == 2`) marks the
        // implied constraint as soft; softness inside compound
        // constraints is not modeled yet, so it parses transparently.
        if self.at_keyword("soft") {
            self.bump();
            let inner = self.parse_unary()?;
            let end = inner.span().1;
            return Some(Expr::Paren {
                expr: Box::new(inner),
                span: (start, end),
            });
        }
        let op = match self.peek_kind() {
            Some(TokenKind::Punctuation(p)) => match p.as_str() {
                "!" => Some(UnaryOp::Not),
                "-" => Some(UnaryOp::Negate),
                "+" => Some(UnaryOp::Plus),
                "~" => Some(UnaryOp::BitNot),
                _ => None,
            },
            _ => None,
        };
        if let Some(op) = op {
            self.bump();
            let inner = self.parse_unary()?;
            let end = inner.span().1;
            return Some(Expr::Unary {
                op,
                expr: Box::new(inner),
                span: (start, end),
            });
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Option<Expr> {
        let (start, end) = self.current_span();
        // `new [<type>] [with { assignments }]` — instance creation
        // (clause 6.2). The type is optional (`var p: packet = new;`).
        // Evaluation is staged: instances come from declared fields and
        // typed locals today.
        if self.at_keyword("new") {
            self.bump();
            let ty = if self.at_identifier()
                || self.at_punct("<")
                || matches!(self.peek_kind(), Some(TokenKind::Keyword(k)) if {
                    ScalarBase::from_keyword(k).is_some() || k == "list"
                }) {
                Some(self.parse_type()?)
            } else {
                None
            };
            let inits = if self.eat_keyword("with") {
                self.expect_punct("{", "E0106", "expected '{' to open the 'with' block")?;
                let inits = self.parse_statements();
                self.expect_punct("}", "E0107", "expected '}' to close the 'with' block")?;
                inits
            } else {
                Vec::new()
            };
            let end = self.last_end();
            let new_expr = Expr::New {
                ty,
                inits,
                span: (start, end),
            };
            return self.parse_postfix_chain(new_expr, start);
        }
        // `select { <weight> : <choice>; .. }` — a weighted distribution
        // (clause 10). Weights parse; choices are recorded verbatim
        // (paren-aware, so `normal(a, b)` keeps its comma and inner
        // tokens). The sampler treats select constraints as no-ops until
        // the distribution arc.
        if self.at_keyword("select") {
            self.bump();
            self.expect_punct("{", "E0106", "expected '{' to open the select block")?;
            let mut arms = Vec::new();
            while !self.at_punct("}") && self.peek().is_some() {
                if self.at_punct(";") {
                    self.bump();
                    continue;
                }
                let weight = self.parse_expr()?;
                self.expect_punct(":", "E0103", "expected ':' after the select weight")?;
                let mut choice = Vec::new();
                let mut depth = 0usize;
                while let Some(token) = self.peek() {
                    match &token.kind {
                        TokenKind::Punctuation(p) if p == ";" && depth == 0 => break,
                        TokenKind::Punctuation(p) if p == "}" && depth == 0 => break,
                        TokenKind::Punctuation(p) if p == "(" => {
                            depth += 1;
                            choice.push(self.bump());
                        }
                        TokenKind::Punctuation(p) if p == ")" => {
                            depth = depth.saturating_sub(1);
                            choice.push(self.bump());
                        }
                        _ => choice.push(self.bump()),
                    }
                }
                if choice.is_empty() {
                    self.error_here("E0108", "expected the select choice");
                    return None;
                }
                self.eat_punct(";");
                arms.push(crate::ast::SelectArm { weight, choice });
            }
            self.expect_punct("}", "E0107", "expected '}' to close the select block")?;
            let end = self.last_end();
            let select = Expr::Select {
                arms,
                span: (start, end),
            };
            return self.parse_postfix_chain(select, start);
        }
        // Predefined e names that appear as bare values in real code
        // (see PREDEFINED_NAMES); treated as plain identifiers in
        // expression position.
        if let Some(TokenKind::Keyword(word)) = self.peek_kind()
            && is_predefined_name(word)
        {
            let name = word.clone();
            self.bump();
            let ident = Expr::Ident {
                name,
                span: (start, end),
            };
            // These names take the same postfix chain as identifiers
            // (`it.a`, `append(s, index, …)` — call args parse through
            // here too).
            return self.parse_postfix_chain(ident, start);
        }
        match self.peek_kind() {
            Some(TokenKind::Number(_)) => {
                let token = self.bump();
                let TokenKind::Number(text) = token.kind else {
                    unreachable!("peeked a number");
                };
                let expr = if text.contains('\'') {
                    Expr::BitLiteral {
                        text,
                        span: (start, end),
                    }
                } else if looks_like_real(&text) {
                    Expr::RealLiteral {
                        text,
                        span: (start, end),
                    }
                } else {
                    Expr::IntLiteral {
                        text,
                        span: (start, end),
                    }
                };
                Some(expr)
            }
            Some(TokenKind::StringLiteral(_)) => {
                let token = self.bump();
                let TokenKind::StringLiteral(value) = token.kind else {
                    unreachable!("peeked a string");
                };
                Some(Expr::StringLiteral {
                    value,
                    span: (start, end),
                })
            }
            Some(TokenKind::Keyword(k))
                if k == "TRUE" || k == "FALSE" || k == "true" || k == "false" =>
            {
                let value = k == "TRUE" || k == "true";
                self.bump();
                Some(Expr::BoolLiteral {
                    value,
                    span: (start, end),
                })
            }
            Some(TokenKind::Identifier(_)) => {
                let (name, _) = self.take_identifier();
                let expr = Expr::Ident {
                    name,
                    span: (start, end),
                };
                // Postfix chain: member access, calls, indexing.
                self.parse_postfix_chain(expr, start)
            }
            Some(TokenKind::Punctuation(p)) if p == "'" => {
                // Quoted hierarchical HDL path: 'path.to.signal'.
                self.bump();
                let (first, _) = self.expect_identifier(
                    "E0102",
                    "expected a path segment after the opening quote",
                )?;
                let mut path = first;
                loop {
                    if self.at_punct(".") {
                        self.bump();
                        let (segment, _) =
                            self.expect_identifier("E0102", "expected a path segment after '.'")?;
                        path.push('.');
                        path.push_str(&segment);
                    } else if self.at_punct("'") {
                        self.bump();
                        break;
                    } else {
                        self.error_here(
                            "E0102",
                            "expected '.' or a closing quote in the quoted path",
                        );
                        return None;
                    }
                }
                let end = self.last_end();
                Some(Expr::HdlPath {
                    path,
                    span: (start, end),
                })
            }
            Some(TokenKind::Punctuation(p)) if p == "(" => {
                self.bump();
                let inner = self.parse_expr()?;
                self.expect_punct(")", "E0110", "expected ')' to close the expression")?;
                let end = self.last_end();
                let paren = Expr::Paren {
                    expr: Box::new(inner),
                    span: (start, end),
                };
                // `(a == 1).as_a(bool)` — parenthesized primaries take
                // the same postfix chain as identifiers.
                self.parse_postfix_chain(paren, start)
            }
            _ => {
                self.error_here("E0108", "expected an expression");
                None
            }
        }
    }
}

/// The dotted path of an identifier/member-access chain, for naming
/// cover groups by their sampling event.
fn render_expr_path(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident { name, .. } => Some(name.clone()),
        Expr::MemberAccess { object, name, .. } => {
            let mut prefix = render_expr_path(object)?;
            prefix.push('.');
            prefix.push_str(name);
            Some(prefix)
        }
        _ => None,
    }
}

/// Parse one standalone temporal expression (clause 12) — the grammar
/// `expect`/`assume` members use. A tool for tests and future `wait`
/// integration; returns the first diagnostic's text on failure.
pub fn parse_temporal_expression(source: &str) -> Result<crate::ast::TemporalExpr, String> {
    let tokens = match lexer::tokenize(source) {
        Ok(tokens) => tokens,
        Err(err) => return Err(err.message.to_owned()),
    };
    let eof = tokens.last().map(|t| t.end).unwrap_or(Position {
        byte: source.len(),
        line: 1,
        column: 1,
    });
    let mut parser = Parser::new(tokens, eof);
    let te = parser.parse_temporal_expr().ok_or_else(|| {
        parser
            .diagnostics
            .first()
            .map(|d| d.render())
            .unwrap_or_else(|| "expected a temporal expression".to_owned())
    })?;
    if parser.peek().is_some() {
        parser.error_here("E0120", "unexpected token after the temporal expression");
        return Err(parser
            .diagnostics
            .last()
            .map(|d| d.render())
            .unwrap_or_default());
    }
    Ok(te)
}
