//! Recursive-descent parser for the declared subset of the e language.
//!
//! Supported: `struct` / `unit` / `extend` declarations with optional
//! `like` inheritance, field declarations with scalar (`uint(bits:
//! 8)`-style widths included), `list of`, and named types, and `keep`
//! constraint expressions with C-style precedence. Field name groups
//! (`a, b: uint;`) are explicitly rejected with a diagnostic for now.
//!
//! Everything else (methods, `when`, events, temporal expressions, coverage,
//! statements, `soft` constraints) is a later roadmap stage: the parser
//! reports an explicit diagnostic and recovers at declaration or member
//! boundaries. Under errors the AST is partial — constructs that failed to
//! parse are omitted, and the recorded diagnostics are authoritative.
//!
//! ## Diagnostic codes
//!
//! - `E0001` lexical error (converted from [`crate::lexer::LexError`])
//! - `E0101` expected a top-level declaration (`struct`/`unit`/`extend`)
//! - `E0102` expected an identifier
//! - `E0103` expected `:`
//! - `E0104` expected a type
//! - `E0105` expected `;`
//! - `E0106` expected `{`
//! - `E0107` expected `}`
//! - `E0108` expected an expression
//! - `E0109` reserved for a future `(` expectation
//! - `E0110` expected `)`
//! - `E0111` expected `bits` in a width specification
//! - `E0112` expected `of` after `list`
//! - `E0113` unsupported member kind
//! - `E0114` unsupported construct
//! - `E0201` duplicate declaration name
//! - `E0202` duplicate field name
//! - `E0203` unknown type name
//! - `E0204` unknown field referenced in a `keep` constraint
//! - `E0205` width must be positive

use crate::ast::{
    BinaryOp, Declaration, DeclarationKind, Expr, Field, KeepConstraint, Member, ScalarBase,
    TypeExpr, UnaryOp,
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
                    if matches!(k.as_str(), "struct" | "unit" | "extend") =>
                {
                    k.clone()
                }
                _ => {
                    self.error_here("E0101", "expected 'struct', 'unit' or 'extend' declaration");
                    self.recover_to_declaration_boundary();
                    continue;
                }
            };
            if let Some(declaration) = self.parse_declaration(&keyword) {
                declarations.push(declaration);
            }
        }
        declarations
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

    /// One member often produces several AST members (`a, b: uint;` is two
    /// fields), hence the vector.
    fn parse_member(&mut self) -> Option<Vec<Member>> {
        match self.peek_kind() {
            Some(TokenKind::Identifier(_)) => {
                self.parse_field().map(Member::Field).map(|m| vec![m])
            }
            Some(TokenKind::Keyword(k)) if k == "keep" => {
                self.parse_keep().map(|c| vec![Member::Constraint(c)])
            }
            _ => {
                self.error_here(
                    "E0113",
                    "expected a field or 'keep' declaration (other members are not supported yet)",
                );
                self.recover_to_member_boundary();
                None
            }
        }
    }

    fn parse_field(&mut self) -> Option<Field> {
        let (name, name_span) = self.expect_identifier("E0102", "expected field name")?;
        if self.eat_punct(",") {
            self.error_here(
                "E0114",
                "field name groups ('a, b: uint;') are not supported yet; declare each field separately",
            );
            self.recover_to_member_boundary();
            return None;
        }
        self.expect_punct(":", "E0103", "expected ':' after field name")?;
        let ty = self.parse_type()?;
        self.expect_punct(";", "E0105", "expected ';' after field declaration")?;
        Some(Field {
            name,
            name_span,
            ty,
        })
    }

    fn parse_keep(&mut self) -> Option<KeepConstraint> {
        let start = self.current_span().0;
        self.bump(); // 'keep'
        if self.at_keyword("soft") {
            self.error_here("E0114", "soft constraints are not supported yet");
            self.recover_to_member_boundary();
            return None;
        }
        let expr = self.parse_expr()?;
        self.expect_punct(";", "E0105", "expected ';' after keep constraint")?;
        let end = self.last_end();
        Some(KeepConstraint {
            expr,
            span: (start, end),
        })
    }

    fn parse_type(&mut self) -> Option<TypeExpr> {
        let (start, _) = self.current_span();
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

    // ---- expressions, loosest to tightest binding ----

    fn parse_expr(&mut self) -> Option<Expr> {
        self.parse_binary_level(0)
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
                } else if text.contains('.') || text.contains(['e', 'E']) {
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
            Some(TokenKind::Keyword(k)) if k == "TRUE" || k == "FALSE" => {
                let value = k == "TRUE";
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
                while self.at_punct(".") {
                    self.bump();
                    let (member, member_span) =
                        self.expect_identifier("E0102", "expected member name after '.'")?;
                    if self.at_punct("(") {
                        self.error_here("E0114", "method calls are not supported yet");
                        return None;
                    }
                    let end = member_span.1;
                    expr = Expr::MemberAccess {
                        object: Box::new(expr),
                        name: member,
                        name_span: member_span,
                        span: (start, end),
                    };
                }
                Some(expr)
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
