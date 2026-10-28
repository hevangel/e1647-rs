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
//! - `E0201` duplicate declaration name
//! - `E0202` duplicate field name
//! - `E0203` unknown type name
//! - `E0204` unknown identifier referenced in a `keep` constraint
//! - `E0205` width must be positive
//! - `E0206` duplicate name in an inner scope (parameter, local variable or
//!   enumeral)

use crate::ast::{
    BinaryOp, Declaration, DeclarationKind, Enumeral, Expr, Field, KeepConstraint, Member,
    MethodDecl, MethodModifier, Param, ScalarBase, Stmt, SubtypeModifier, TypeExpr, UnaryOp,
    WhenDecl,
};
use crate::diagnostic::Diagnostic;
use crate::lexer::{self, Position, Token, TokenKind};

/// Binary operators from loosest to tightest binding (C-style precedence;
/// all left-associative in this subset).
const BINARY_LEVELS: &[&[(&str, BinaryOp)]] = &[
    &[("||", BinaryOp::LogicalOr)],
    &[("&&", BinaryOp::LogicalAnd)],
    &[("|", BinaryOp::BitOr)],
    &[("^", BinaryOp::BitXor)],
    &[("&", BinaryOp::BitAnd)],
    &[
        ("==", BinaryOp::Eq),
        ("!=", BinaryOp::NotEq),
        ("===", BinaryOp::IdentityEq),
        ("!==", BinaryOp::IdentityNe),
    ],
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
}

impl Parser {
    fn new(tokens: Vec<Token>, eof: Position) -> Self {
        Self {
            tokens,
            pos: 0,
            eof,
            diagnostics: Vec::new(),
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

    fn parse_program(&mut self) -> Vec<Declaration> {
        let mut declarations = Vec::new();
        while self.peek().is_some() {
            let keyword = match self.peek_kind() {
                Some(TokenKind::Keyword(k))
                    if matches!(k.as_str(), "struct" | "unit" | "extend" | "type") =>
                {
                    k.clone()
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
                self.parse_type_declaration()
            } else {
                self.parse_declaration(&keyword)
            };
            if let Some(declaration) = declaration {
                declarations.push(declaration);
            }
        }
        declarations
    }

    /// `type name: [ENUMERAL [= expr] , ..];`
    fn parse_type_declaration(&mut self) -> Option<Declaration> {
        let start = self.current_span().0;
        self.bump(); // 'type'
        let (name, name_span) = self.expect_identifier("E0102", "expected type name")?;
        self.expect_punct(":", "E0103", "expected ':' after the type name")?;
        if self
            .expect_punct("[", "E0106", "expected '[' to open the enumeral list")
            .is_none()
        {
            self.recover_to_declaration_boundary();
            return Some(Declaration {
                kind: DeclarationKind::Type,
                name,
                name_span,
                like: None,
                like_span: None,
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
                like: None,
                like_span: None,
                members,
                span: (start, self.last_end()),
            });
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
                like: None,
                like_span: None,
                members,
                span: (start, self.last_end()),
            });
        }
        let end = self.last_end();
        Some(Declaration {
            kind: DeclarationKind::Type,
            name,
            name_span,
            like: None,
            like_span: None,
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
        let (name, name_span) = self.expect_identifier("E0102", "expected declaration name")?;
        let mut like = None;
        let mut like_span = None;
        if self.eat_keyword("like") {
            let (base, span) =
                self.expect_identifier("E0102", "expected type name after 'like'")?;
            like = Some(base);
            like_span = Some(span);
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
                like,
                like_span,
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
            like,
            like_span,
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
        let (first, first_span) = self.expect_identifier("E0102", "expected field name")?;
        let mut names = vec![(first, first_span)];
        while self.eat_punct(",") {
            let (name, name_span) =
                self.expect_identifier("E0102", "expected field name after ','")?;
            names.push((name, name_span));
        }
        self.expect_punct(":", "E0103", "expected ':' after field name")?;
        let ty = self.parse_type()?;
        // `is instance` / `is inline` suffixes are syntactic in this
        // subset.
        if self.eat_keyword("is") && !self.eat_keyword("instance") && !self.eat_keyword("inline") {
            self.error_here("E0104", "expected 'instance' or 'inline' after 'is'");
            return None;
        }
        self.expect_punct(";", "E0105", "expected ';' after field declaration")?;
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
                    })
                })
                .collect(),
        )
    }

    /// `name(params): return_type is [also|first|only] { statements };`
    fn parse_method(&mut self) -> Option<Member> {
        let start = self.current_span().0;
        let (name, name_span) = self.take_identifier();
        self.expect_punct(
            "(",
            "E0109",
            "expected '(' to open the method parameter list",
        )?;
        let mut params = Vec::new();
        if !self.at_punct(")") {
            loop {
                let (param_name, param_span) =
                    self.expect_identifier("E0102", "expected parameter name")?;
                self.expect_punct(":", "E0103", "expected ':' after parameter name")?;
                let ty = self.parse_type()?;
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
        self.expect_punct("{", "E0106", "expected '{' to open the method body")?;
        let body = self.parse_statements();
        self.expect_punct("}", "E0107", "expected '}' to close the method body")?;
        self.expect_punct(";", "E0105", "expected ';' after the method declaration")?;
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
            kind: TokenKind::Identifier(direction),
            ..
        }) = self.tokens.get(self.pos + 2)
        else {
            return false;
        };
        matches!(direction.as_str(), "in" | "out" | "inout")
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
            Some(TokenKind::Identifier(id)) => match id.as_str() {
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

    /// `event name;` or `event name is <trigger>;` — the trigger
    /// expression is stored as written (e.g. `fall(clk$) @sim`,
    /// `cycle @p_smp.clock_f`, `@sys.any`); edge scheduling is a later
    /// milestone.
    fn parse_event(&mut self) -> Option<Member> {
        let start = self.current_span().0;
        self.bump(); // 'event'
        let (name, name_span) = self.expect_identifier("E0102", "expected event name")?;
        let trigger = if self.eat_keyword("is") {
            Some(self.parse_expr()?)
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

    fn parse_keep(&mut self) -> Option<KeepConstraint> {
        let start = self.current_span().0;
        self.bump(); // 'keep'
        let is_soft = self.eat_keyword("soft");
        let expr = self.parse_expr()?;
        self.expect_punct(";", "E0105", "expected ';' after keep constraint")?;
        let end = self.last_end();
        Some(KeepConstraint {
            expr,
            is_soft,
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
        if self.at_identifier() {
            let (name, span) = self.take_identifier();
            return Some(TypeExpr::Named { name, span });
        }
        let Some(TokenKind::Keyword(keyword)) = self.peek_kind() else {
            self.error_here("E0104", "expected a type");
            return None;
        };
        let keyword = keyword.clone();
        if keyword == "list" {
            self.bump();
            if !self.eat_keyword("of") {
                self.error_here("E0112", "expected 'of' after 'list'");
                return None;
            }
            let element = Box::new(self.parse_type()?);
            let end = element.span().1;
            return Some(TypeExpr::List {
                element,
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
        self.error_here("E0104", "expected a type");
        None
    }

    // ---- statements ----

    /// Statements up to (not including) the closing `}` of the enclosing
    /// block.
    fn parse_statements(&mut self) -> Vec<Stmt> {
        let mut statements = Vec::new();
        while self.peek().is_some() && !self.at_punct("}") {
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
                let (name, name_span) =
                    self.expect_identifier("E0102", "expected variable name")?;
                self.expect_punct(":", "E0103", "expected ':' after the variable name")?;
                let ty = self.parse_type()?;
                let init = if self.eat_punct("=") {
                    Some(self.parse_expr()?)
                } else {
                    None
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
                self.expect_punct(";", "E0105", "expected ';' after 'return'")?;
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
                self.expect_punct(";", "E0105", "expected ';' after 'gen'")?;
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
                self.expect_punct(";", "E0105", "expected ';' after 'emit'")?;
                let end = self.last_end();
                Some(Stmt::Emit {
                    target,
                    span: (start, end),
                })
            }
            Some(TokenKind::Keyword(k)) if k == "start" => {
                self.bump();
                let call = self.parse_expr()?;
                self.expect_punct(";", "E0105", "expected ';' after 'start'")?;
                let end = self.last_end();
                Some(Stmt::Start {
                    call,
                    span: (start, end),
                })
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
                    self.expect_punct(";", "E0105", "expected ';' after assignment")?;
                    let end = self.last_end();
                    Some(Stmt::Assign {
                        target: expr,
                        value,
                        span: (start, end),
                    })
                } else {
                    self.expect_punct(";", "E0105", "expected ';' after statement")?;
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
        } else {
            None
        };
        self.expect_punct(";", "E0105", "expected ';' after the 'if' statement")?;
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
        self.expect_punct(";", "E0105", "expected ';' after the 'while' statement")?;
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
        self.expect_punct(";", "E0105", "expected ';' after 'wait'")?;
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
            self.expect_keyword_word("in")?;
            let list = self.parse_expr()?;
            self.expect_keyword_word("do")?;
            self.expect_punct("{", "E0106", "expected '{' to open the for-each body")?;
            let body = self.parse_statements();
            self.expect_punct("}", "E0107", "expected '}' to close the for-each body")?;
            self.expect_punct(";", "E0105", "expected ';' after the for-each statement")?;
            let end = self.last_end();
            return Some(Stmt::ForEach {
                list,
                body,
                span: (start, end),
            });
        }
        let (var, _) = self.expect_identifier("E0102", "expected the loop variable after 'for'")?;
        self.expect_keyword_word("from")?;
        let from = self.parse_expr()?;
        self.expect_keyword_word("to")?;
        let to = self.parse_expr()?;
        self.expect_punct("{", "E0106", "expected '{' to open the for body")?;
        let body = self.parse_statements();
        self.expect_punct("}", "E0107", "expected '}' to close the for body")?;
        self.expect_punct(";", "E0105", "expected ';' after the for statement")?;
        let end = self.last_end();
        Some(Stmt::ForTo {
            var,
            from,
            to,
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

    // ---- expressions, loosest to tightest binding ----

    fn parse_expr(&mut self) -> Option<Expr> {
        let first = self.parse_binary_level(0)?;
        // `expr in [a, b]` / `expr in [low .. high]` membership.
        if self.eat_keyword("in") {
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
            let start = first.span().0;
            let end = self.last_end();
            return Some(Expr::InSet {
                element: Box::new(first),
                set,
                span: (start, end),
            });
        }
        if self.eat_punct("?") {
            let then_expr = self.parse_expr()?;
            self.expect_punct(":", "E0103", "expected ':' in the ternary expression")?;
            let else_expr = self.parse_expr()?;
            let start = first.span().0;
            let end = else_expr.span().1;
            return Some(Expr::Ternary {
                cond: Box::new(first),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
                span: (start, end),
            });
        }
        Some(first)
    }

    fn parse_binary_level(&mut self, level: usize) -> Option<Expr> {
        let Some(operators) = BINARY_LEVELS.get(level) else {
            return self.parse_unary();
        };
        let mut lhs = self.parse_binary_level(level + 1)?;
        loop {
            let op = match self.peek_kind() {
                Some(TokenKind::Punctuation(symbol)) => operators
                    .iter()
                    .find(|(text, _)| text == symbol)
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
                let mut expr = Expr::Ident {
                    name,
                    span: (start, end),
                };
                // Postfix chain: member access, calls, indexing.
                loop {
                    if self.at_punct(".") {
                        self.bump();
                        let (member, member_span) =
                            self.expect_identifier("E0102", "expected member name after '.'")?;
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
                        self.expect_punct(
                            ")",
                            "E0110",
                            "expected ')' to close the call arguments",
                        )?;
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
                            self.expect_punct(
                                "]",
                                "E0107",
                                "expected ']' to close the index expression",
                            )?;
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
                Some(Expr::Paren {
                    expr: Box::new(inner),
                    span: (start, end),
                })
            }
            _ => {
                self.error_here("E0108", "expected an expression");
                None
            }
        }
    }
}
