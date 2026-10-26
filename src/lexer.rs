//! A deliberately small, dependency-free tokenizer for an early `e` subset.
//! It does not yet claim complete lexical compliance with IEEE 1647.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Identifier(String),
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
                _ => return Ok(()),
            }
        }
    }

    fn next_token(&mut self) -> Result<Option<Token>, LexError> {
        self.skip_trivia()?;
        let start = self.position();
        let Some(first) = self.peek() else {
            return Ok(None);
        };
        let kind = if first.is_ascii_alphabetic() || first == '_' {
            let start_byte = self.byte;
            while self
                .peek()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                self.bump();
            }
            TokenKind::Identifier(self.source[start_byte..self.byte].to_owned())
        } else if first.is_ascii_digit() {
            let start_byte = self.byte;
            while self
                .peek()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                self.bump();
            }
            TokenKind::Number(self.source[start_byte..self.byte].to_owned())
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
        } else {
            let mut symbol = String::new();
            symbol.push(self.bump().expect("peeked a character"));
            if matches!(first, '<' | '>' | '=' | '!') && self.peek() == Some('=') {
                symbol.push(self.bump().expect("peeked equals"));
            } else if matches!(first, '&' | '|') && self.peek() == Some(first) {
                symbol.push(self.bump().expect("peeked matching operator"));
            }
            TokenKind::Punctuation(symbol)
        };
        Ok(Some(Token {
            kind,
            start,
            end: self.position(),
        }))
    }
}

/// Tokenize source. Future parser versions will perform keyword recognition,
/// full numeral/bit-literal handling, and semantic validation.
pub fn tokenize(source: &str) -> Result<Vec<Token>, LexError> {
    let mut cursor = Cursor::new(source);
    let mut result = Vec::new();
    while let Some(token) = cursor.next_token()? {
        result.push(token);
    }
    Ok(result)
}
