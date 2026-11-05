//! The macro engine (clause 15): match-expression parsing (15.4),
//! simple-macro expansion (15.5/15.6) and computed-macro evaluation
//! (15.3) as a pre-pass over the token stream.
//!
//! A macro definition (`define <name'category> "pattern" as [computed]
//! { body };`) associates a new production with the non-terminal named
//! by its category. This module compiles the recorded pattern strings
//! into a term tree (15.4: escaped literal terminals, `<[tag']type>`
//! non-terminals, `[ ]` option, `( )` grouping, `|` alternation),
//! matches token sequences against them with backtracking (15.5.2's
//! recursive-descent reading), and expands the body: a simple macro's
//! body splices the matched subsequence at the parameter references
//! (15.6: "the expansion segment is taken literally, except for
//! occurrences of syntactic parameters"); a computed macro's body is
//! evaluated at match time by a restricted parse-time interpreter
//! (15.3: "the block of actions is treated as the body of a method
//! that returns a string") and the returned string tokenizes into the
//! expansion.
//!
//! The pass runs before the parser reads the token stream, so
//! `MIN_VALUE(addr_t)`-style invocation sites anywhere in ordinary
//! code become their body text and parse like any other code. Later
//! definitions have higher priority (15.5.1); the first macro whose
//! whole pattern matches wins, and a computed definition that declines
//! (a `reject_match()` action, an unevaluable body, or an illegal
//! result) falls through to the next lower-priority definition — the
//! type-min-max corpus's define-as-computed/define-as pair relies on
//! that order. V1 approximations, documented: a literal terminal
//! matches one token whose whole text equals it (whitespace-only
//! literals vanish — tokenization ignores whitespace, so the `[ ]`
//! optional-space form is vacuous); non-terminals consume by
//! backtracking over split lengths; the `<type, ...>` repetition form
//! matches as one non-terminal; the computed-body interpreter reads
//! only a restricted statement subset (see `evaluate_computed_body`)
//! — method-style calls, `is a` checks, `else if` chains, and
//! list/number expressions are not evaluated, and such bodies decline;
//! pattern-argument categories are not validated.

use std::collections::HashMap;

use crate::lexer::{Token, TokenKind};

/// One term of a compiled match expression (15.4).
#[derive(Debug, Clone, PartialEq, Eq)]
enum MatchTerm {
    /// A literal terminal: matches one token whose whole text equals
    /// it (15.4.2; whitespace-only literals are dropped at compile
    /// time).
    Literal(String),
    /// `<[tag']kind>` — a syntactic argument (15.4.2) that consumes a
    /// subsequence of tokens by backtracking.
    NonTerminal { tag: Option<String>, kind: String },
    /// `[ ... ]` (optional) or `( ... )` (grouping) with `|`
    /// alternatives (15.4.3); an option succeeds even when nothing
    /// matches.
    Alternatives {
        optional: bool,
        branches: Vec<Vec<MatchTerm>>,
    },
}

/// One scanned definition: the compiled pattern and the body tokens.
struct ScannedMacro {
    terms: Vec<MatchTerm>,
    computed: bool,
    body: Vec<Token>,
}

/// Compile a recorded pattern string into terms. `None` on a pattern
/// this compiler cannot read (the macro is then inert — documented at
/// the use site).
fn compile_pattern(pattern: &str) -> Option<Vec<MatchTerm>> {
    let characters: Vec<char> = pattern.chars().collect();
    let mut cursor = 0usize;
    let terms = parse_terms(&characters, &mut cursor, true)?;
    if cursor != characters.len() {
        return None;
    }
    Some(terms)
}

/// Parse terms until the closing bracket of the current nesting level
/// (`top` = the whole string).
fn parse_terms(characters: &[char], cursor: &mut usize, top: bool) -> Option<Vec<MatchTerm>> {
    let mut terms = Vec::new();
    let mut literal = String::new();
    while *cursor < characters.len() {
        let character = characters[*cursor];
        match character {
            '\\' if *cursor + 1 < characters.len() => {
                // An escaped character is a standalone literal terminal
                // (it lexes as its own punctuation token — `\(` is the
                // token `(`, never part of a longer word).
                push_literal(&mut literal, &mut terms);
                literal.push(characters[*cursor + 1]);
                push_literal(&mut literal, &mut terms);
                *cursor += 2;
            }
            '[' | '(' => {
                push_literal(&mut literal, &mut terms);
                let close = if character == '[' { ']' } else { ')' };
                *cursor += 1;
                let branches = parse_alternatives(characters, cursor, close)?;
                if *cursor >= characters.len() || characters[*cursor] != close {
                    return None;
                }
                *cursor += 1;
                terms.push(MatchTerm::Alternatives {
                    optional: character == '[',
                    branches,
                });
            }
            ']' | ')' if !top => {
                push_literal(&mut literal, &mut terms);
                return Some(terms);
            }
            ']' | ')' | '>' | '|' => return None,
            '<' => {
                push_literal(&mut literal, &mut terms);
                let term = parse_non_terminal(characters, cursor)?;
                terms.push(term);
            }
            _ => {
                literal.push(character);
                *cursor += 1;
            }
        }
    }
    if top {
        push_literal(&mut literal, &mut terms);
        Some(terms)
    } else {
        None
    }
}

/// Parse `alt | alt | alt` until the given closing bracket (not
/// consumed).
fn parse_alternatives(
    characters: &[char],
    cursor: &mut usize,
    close: char,
) -> Option<Vec<Vec<MatchTerm>>> {
    let mut branches = Vec::new();
    let mut current = Vec::new();
    let mut literal = String::new();
    while *cursor < characters.len() {
        let character = characters[*cursor];
        match character {
            c if c == close => {
                push_literal(&mut literal, &mut current);
                if !current.is_empty() {
                    branches.push(current);
                }
                return Some(branches);
            }
            '|' => {
                push_literal(&mut literal, &mut current);
                branches.push(std::mem::take(&mut current));
                *cursor += 1;
            }
            '\\' if *cursor + 1 < characters.len() => {
                // Escaped characters stand alone (see parse_terms).
                push_literal(&mut literal, &mut current);
                literal.push(characters[*cursor + 1]);
                push_literal(&mut literal, &mut current);
                *cursor += 2;
            }
            '<' => {
                push_literal(&mut literal, &mut current);
                let term = parse_non_terminal(characters, cursor)?;
                current.push(term);
            }
            '[' | '(' => {
                push_literal(&mut literal, &mut current);
                let inner_close = if character == '[' { ']' } else { ')' };
                *cursor += 1;
                let nested = parse_alternatives(characters, cursor, inner_close)?;
                if *cursor >= characters.len() || characters[*cursor] != inner_close {
                    return None;
                }
                *cursor += 1;
                current.push(MatchTerm::Alternatives {
                    optional: character == '[',
                    branches: nested,
                });
            }
            _ => {
                literal.push(character);
                *cursor += 1;
            }
        }
    }
    None
}

fn push_literal(literal: &mut String, terms: &mut Vec<MatchTerm>) {
    if !literal.is_empty() {
        let text = std::mem::take(literal);
        // Whitespace-only terminals are vacuous over a token stream
        // (the `[ ]` optional-space spelling).
        if text.chars().any(|c| !c.is_whitespace()) {
            terms.push(MatchTerm::Literal(text));
        }
    }
}

/// Parse `<[tag']kind>` at `cursor['<']` (15.6(a); the plural `<...s>`
/// / `<..., ...>` forms match as one non-terminal in this subset).
fn parse_non_terminal(characters: &[char], cursor: &mut usize) -> Option<MatchTerm> {
    *cursor += 1; // '<'
    let mut inner = String::new();
    while *cursor < characters.len() && characters[*cursor] != '>' {
        inner.push(characters[*cursor]);
        *cursor += 1;
    }
    if *cursor >= characters.len() {
        return None;
    }
    *cursor += 1; // '>'
    let inner = inner.strip_suffix(",...").unwrap_or(&inner);
    let (tag, kind) = match inner.split_once('\'') {
        Some((tag, kind)) => (Some(tag.to_owned()), kind.to_owned()),
        None => (None, inner.to_owned()),
    };
    let kind = kind.strip_suffix('s').unwrap_or(&kind).to_owned();
    if kind.is_empty() {
        return None;
    }
    Some(MatchTerm::NonTerminal { tag, kind })
}

/// The text a terminal matches against: a punctuation token its
/// symbol, anything else its raw text.
fn token_text(token: &Token) -> &str {
    match &token.kind {
        TokenKind::Punctuation(symbol) => symbol,
        TokenKind::Identifier(word) | TokenKind::Keyword(word) => word,
        TokenKind::Number(text) | TokenKind::StringLiteral(text) => text,
    }
}

/// Match `terms` against the tokens from `cursor`; on success advance
/// `cursor` past the match and append the non-terminals' submatches to
/// `parameters` (15.6: "their actual values are the corresponding
/// string part of the input text"). Backtracking over non-terminal
/// lengths, longest first.
fn match_terms(
    terms: &[MatchTerm],
    tokens: &[Token],
    cursor: &mut usize,
    parameters: &mut Vec<(Option<String>, String, Vec<Token>)>,
    budget: &mut u32,
) -> bool {
    if *budget == 0 {
        return false;
    }
    *budget -= 1;
    let Some((first, rest)) = terms.split_first() else {
        return true; // the empty production
    };
    match first {
        MatchTerm::Literal(text) => {
            let Some(token) = tokens.get(*cursor) else {
                return false;
            };
            if token_text(token) == text {
                *cursor += 1;
                match_terms(rest, tokens, cursor, parameters, budget)
            } else {
                false
            }
        }
        MatchTerm::NonTerminal { tag, kind } => {
            // The bounded end: scan forward tracking bracket depth, and
            // stop before a depth-0 closing bracket or separator — the
            // delimiters of one syntactic argument (the `)` of the
            // enclosing pattern literal, a `,` of a repetition). The
            // tail terms must then match what remains, longest split
            // first.
            let start = *cursor;
            let mut depth = 0i32;
            let mut bounded = start;
            while bounded < tokens.len() {
                let text = token_text(&tokens[bounded]).to_owned();
                match text.as_str() {
                    "(" | "[" | "{" => {
                        depth += 1;
                        bounded += 1;
                    }
                    ")" | "]" | "}" => {
                        if depth == 0 {
                            break;
                        }
                        depth -= 1;
                        bounded += 1;
                    }
                    "," | ";" if depth == 0 => break,
                    _ => bounded += 1,
                }
            }
            let mut end = bounded;
            loop {
                if *budget == 0 {
                    return false;
                }
                *budget -= 1;
                let saved = *cursor;
                let saved_parameters = parameters.clone();
                *cursor = end;
                if match_terms(rest, tokens, cursor, parameters, budget) {
                    parameters.push((tag.clone(), kind.clone(), tokens[start..end].to_vec()));
                    return true;
                }
                *cursor = saved;
                *parameters = saved_parameters;
                if end == start {
                    break;
                }
                end -= 1;
            }
            false
        }
        MatchTerm::Alternatives { optional, branches } => {
            for branch in branches {
                let saved = *cursor;
                let saved_parameters = parameters.clone();
                if match_terms(branch, tokens, cursor, parameters, budget)
                    && match_terms(rest, tokens, cursor, parameters, budget)
                {
                    return true;
                }
                *cursor = saved;
                *parameters = saved_parameters;
            }
            if *optional {
                match_terms(rest, tokens, cursor, parameters, budget)
            } else {
                false
            }
        }
    }
}

/// One macro invocation expanded: the substituted body tokens.
struct Expansion {
    tokens: Vec<Token>,
}

/// The anchor a macro's pattern starts with — the pass only attempts
/// matching where this text appears (patterns that do not start with a
/// literal are inert in this subset).
fn anchor(terms: &[MatchTerm]) -> Option<&str> {
    match terms.first()? {
        MatchTerm::Literal(text) => Some(text),
        _ => None,
    }
}

/// Try the macros at `position` (later definitions first — 15.5.1's
/// priority) and render the expansion with the matched parameters.
/// Returns the expansion and the position after the matched span. A
/// computed definition that declines (see `evaluate_computed_body`)
/// falls through to the next lower-priority definition, per 15.5.2's
/// "backtracking proceeds further".
fn expand_at(
    macros: &[ScannedMacro],
    tokens: &[Token],
    position: usize,
) -> Option<(Expansion, usize)> {
    for scanned in macros.iter().rev() {
        let Some(anchor) = anchor(&scanned.terms) else {
            continue;
        };
        let token = tokens.get(position)?;
        if token_text(token) != anchor {
            continue;
        }
        let mut cursor = position;
        let mut parameters = Vec::new();
        let mut budget = 10_000u32;
        if !match_terms(
            &scanned.terms,
            tokens,
            &mut cursor,
            &mut parameters,
            &mut budget,
        ) {
            continue;
        }
        if scanned.computed {
            let Some(result) = evaluate_computed_body(&scanned.body, &parameters) else {
                // Declined: a `reject_match()` action executed, the body
                // uses a construct the evaluator does not read, or the
                // result is not a legal expansion. The next lower-priority
                // definition (e.g. the file's simple define-as of the same
                // pattern) applies.
                continue;
            };
            let Ok(expansion) = crate::lexer::tokenize(&result) else {
                // 15.3: the returned string must be a legal e construct;
                // one that does not even tokenize declines.
                continue;
            };
            if expansion.is_empty() {
                // A whitespace/comment-only result is not a construct of
                // the category — decline.
                continue;
            }
            // Note: the expansion's token spans are relative to the
            // returned string, not the original source (generated code).
            return Some((Expansion { tokens: expansion }, cursor));
        }
        return Some((
            Expansion {
                tokens: render_body(&scanned.body, &parameters),
            },
            cursor,
        ));
    }
    None
}

/// The whole pre-pass: find the `define <name'category> "pattern" as
/// [computed] { body };` definitions in the token stream, then replace
/// every invocation site outside the definitions with its expansion.
/// Iterates until nothing changes (a body may invoke another macro),
/// capped.
pub fn expand_macros(tokens: Vec<Token>) -> Vec<Token> {
    let macros = scan_definitions(&tokens);
    if macros.is_empty() {
        return tokens;
    }
    let mut current = tokens;
    for _ in 0..10 {
        // Recomputed each iteration: the stream shifts as invocations
        // expand, and a body may itself contain invocations that only
        // become visible after the first round.
        let definition_spans = definition_spans(&current);
        let mut output = Vec::with_capacity(current.len());
        let mut position = 0usize;
        let mut changed = false;
        while position < current.len() {
            if let Some((_, end)) = definition_spans
                .iter()
                .find(|(start, end)| position >= *start && position < *end)
            {
                // Copy the whole definition verbatim (expansion must
                // not rewrite definitions).
                output.extend(current[position..*end].iter().cloned());
                position = *end;
                continue;
            }
            if let Some((expansion, end)) = expand_at(&macros, &current, position) {
                output.extend(expansion.tokens);
                position = end;
                changed = true;
            } else {
                output.push(current[position].clone());
                position += 1;
            }
        }
        current = output;
        if !changed {
            break;
        }
    }
    current
}

/// Scan for the definition token shape: `define` `<` name [`'` category]
/// `>` "pattern" `as` [computed] `{` body `}` `;`.
fn scan_definitions(tokens: &[Token]) -> Vec<ScannedMacro> {
    let mut macros = Vec::new();
    let mut index = 0usize;
    while index < tokens.len() {
        if !matches!(&tokens[index].kind, TokenKind::Keyword(word) if word == "define") {
            index += 1;
            continue;
        }
        let Some(scanned) = scan_one_definition(tokens, index) else {
            index += 1;
            continue;
        };
        index = scanned.end;
        if let Some(terms) = compile_pattern(&scanned.pattern) {
            macros.push(ScannedMacro {
                terms,
                computed: scanned.computed,
                body: scanned.body,
            });
        }
    }
    macros
}

struct ScannedDefinition {
    pattern: String,
    computed: bool,
    body: Vec<Token>,
    end: usize,
}

/// The token kind at `index`, or `None` past the end.
fn kind_at(tokens: &[Token], index: usize) -> Option<&TokenKind> {
    tokens.get(index).map(|token| &token.kind)
}

/// Scan one definition starting at `start['define']`.
fn scan_one_definition(tokens: &[Token], start: usize) -> Option<ScannedDefinition> {
    let mut cursor = start + 1;
    if !matches!(kind_at(tokens, cursor)?, TokenKind::Punctuation(p) if p == "<") {
        return None;
    }
    cursor += 1;
    if !matches!(
        kind_at(tokens, cursor)?,
        TokenKind::Identifier(_) | TokenKind::Keyword(_)
    ) {
        return None;
    }
    cursor += 1;
    if matches!(kind_at(tokens, cursor)?, TokenKind::Punctuation(p) if p == "'") {
        cursor += 1;
        if !matches!(kind_at(tokens, cursor)?, TokenKind::Identifier(_)) {
            return None;
        }
        cursor += 1;
    }
    if !matches!(kind_at(tokens, cursor)?, TokenKind::Punctuation(p) if p == ">") {
        return None;
    }
    cursor += 1;
    let TokenKind::StringLiteral(pattern) = kind_at(tokens, cursor)? else {
        return None;
    };
    let pattern = pattern.clone();
    cursor += 1;
    if !matches!(kind_at(tokens, cursor)?, TokenKind::Keyword(word) if word == "as") {
        return None;
    }
    cursor += 1;
    let computed = matches!(kind_at(tokens, cursor)?, TokenKind::Identifier(w) if w == "computed");
    if computed {
        cursor += 1;
    }
    if !matches!(kind_at(tokens, cursor)?, TokenKind::Punctuation(p) if p == "{") {
        return None;
    }
    cursor += 1;
    let body_start = cursor;
    let mut depth = 1usize;
    while cursor < tokens.len() && depth > 0 {
        match &tokens[cursor].kind {
            TokenKind::Punctuation(p) if p == "{" => depth += 1,
            TokenKind::Punctuation(p) if p == "}" => depth -= 1,
            _ => {}
        }
        cursor += 1;
    }
    if depth != 0 {
        return None;
    }
    // The body's trailing `;` terminates the body segment itself
    // (15.4.6: the semicolon's function is fixed to sequencing inside
    // braces) — it is not part of the expansion; the invocation site
    // carries its own.
    let body_end = cursor - 1;
    let body_trimmed = if body_end > body_start
        && matches!(&tokens[body_end - 1].kind, TokenKind::Punctuation(p) if p == ";")
    {
        body_end - 1
    } else {
        body_end
    };
    let body = tokens[body_start..body_trimmed].to_vec();
    if matches!(kind_at(tokens, cursor)?, TokenKind::Punctuation(p) if p == ";") {
        cursor += 1;
    }
    Some(ScannedDefinition {
        pattern,
        computed,
        body,
        end: cursor,
    })
}

/// The token-index spans of the macro definitions (expansion skips
/// them).
fn definition_spans(tokens: &[Token]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut index = 0usize;
    while index < tokens.len() {
        if !matches!(&tokens[index].kind, TokenKind::Keyword(word) if word == "define") {
            index += 1;
            continue;
        }
        let Some(scanned) = scan_one_definition(tokens, index) else {
            index += 1;
            continue;
        };
        spans.push((index, scanned.end));
        index = scanned.end;
    }
    spans
}

/// Render the body with the parameter references substituted (15.6):
/// a `<` name [`'` category] `>` token sequence in the body becomes
/// the matched subsequence — by tag when the reference carries one,
/// else by the kind when exactly one parameter has it. Unmatched
/// references stay as written.
fn render_body(body: &[Token], parameters: &[(Option<String>, String, Vec<Token>)]) -> Vec<Token> {
    let mut output = Vec::with_capacity(body.len());
    let mut index = 0usize;
    while index < body.len() {
        let is_open = matches!(&body[index].kind, TokenKind::Punctuation(p) if p == "<");
        if !is_open {
            output.push(body[index].clone());
            index += 1;
            continue;
        }
        // Collect the reference's inner tokens up to the '>'.
        let mut inner = Vec::new();
        let mut scan = index + 1;
        let mut closed = false;
        while scan < body.len() {
            if matches!(&body[scan].kind, TokenKind::Punctuation(p) if p == ">") {
                closed = true;
                break;
            }
            inner.push(&body[scan]);
            scan += 1;
        }
        if !closed {
            output.push(body[index].clone());
            index += 1;
            continue;
        }
        if let Some(submatch) = resolve_reference(&inner, parameters) {
            output.extend(submatch.iter().cloned());
        } else {
            output.extend(body[index..=scan].iter().cloned());
        }
        index = scan + 1;
    }
    output
}

/// Resolve one `<...>` reference against the matched parameters.
fn resolve_reference(
    inner: &[&Token],
    parameters: &[(Option<String>, String, Vec<Token>)],
) -> Option<Vec<Token>> {
    // The reference spells `name`, `name'category`, the plural
    // `names`, or `name, ...`; the singular kind is what the pattern
    // declared.
    let mut joined = String::new();
    for token in inner {
        joined.push_str(token_text(token));
    }
    let joined = joined.strip_suffix(", ...").unwrap_or(&joined);
    let joined = joined.strip_suffix(",...").unwrap_or(joined);
    let (tag, kind) = match joined.split_once('\'') {
        Some((tag, kind)) => (
            Some(tag.to_owned()),
            kind.strip_suffix('s').unwrap_or(kind).to_owned(),
        ),
        None => (None, joined.strip_suffix('s').unwrap_or(joined).to_owned()),
    };
    if let Some(tag) = tag {
        return parameters
            .iter()
            .find(|(parameter_tag, _, _)| parameter_tag.as_deref() == Some(&tag))
            .map(|(_, _, tokens)| tokens.clone());
    }
    let matching: Vec<&(Option<String>, String, Vec<Token>)> = parameters
        .iter()
        .filter(|(_, parameter_kind, _)| *parameter_kind == kind)
        .collect();
    if matching.len() == 1 {
        return Some(matching[0].2.clone());
    }
    None
}

// ---------------------------------------------------------------------------
// Clause 15.3 computed bodies: a restricted parse-time interpreter.
//
// 15.3 treats the block of actions as the body of a method that returns
// a string: either the `result` variable or the `return` action is used,
// and the returned string must be a legal e construct of the same
// category. The body executes at parse time whenever the match
// expression matched, with the syntactic parameters available as string
// variables (15.3/15.6); definitions in the same module cannot be
// presupposed by the computation (15.3), so there are no global or
// method lookups here.
//
// The interpreter reads a restricted statement subset: `var name :=
// expr;` (also `var name = expr;` and `var name: type = expr;`),
// assignments, `if cond { .. } [else { .. }]`, `return expr;`, and
// expression statements (`reject_match();`). Expressions are string
// literals, the implicit boolean literals TRUE/FALSE, `<...>` parameter
// references (their matched text as a string), variables (locals and
// parameters by name), `+` concatenation, `==`/`!=` comparisons, parenthesized
// expressions, and calls — of which only `reject_match()` is known.
//
// Anything else — a call to another routine (`rf_manager`, `out`, method
// calls like `s.to_string()`), an `is a` check, an `else if` chain, a
// number/list expression — makes the evaluation fail. A failing body, a
// body executing `reject_match()`, a non-string `return`, and an empty
// or lexically illegal result ALL decline the definition: the matcher
// continues with the next lower-priority definition (15.5.1's order), so
// the type-min-max corpus's define-as-computed bodies (which call
// `rf_manager` and reject non-numeric types) yield to the file's simple
// define-as of the same pattern. This is the honest v1 reading: parse
// time has no type objects to reflect on, so a body that branches on
// reflection cannot succeed here.
// ---------------------------------------------------------------------------

/// One statement of the restricted computed-body subset.
#[derive(Debug, Clone)]
enum CompStmt {
    /// `var name := expr;` (or `=`) and plain `name = expr;` — both set
    /// the local string variable.
    Assign { name: String, expr: CompExpr },
    /// `if cond { .. } [else { .. }]` — only a braced `else`.
    If {
        cond: CompExpr,
        then_body: Vec<CompStmt>,
        else_body: Option<Vec<CompStmt>>,
    },
    /// `return expr;` — a bare `return;` parses but fails evaluation
    /// (15.3's method returns a string).
    Return(Option<CompExpr>),
    /// An expression statement, e.g. `reject_match();`.
    Expr(CompExpr),
}

/// One expression of the restricted computed-body subset.
#[derive(Debug, Clone)]
enum CompExpr {
    /// A string literal token's raw value.
    Str(String),
    /// The implicit boolean literals.
    Bool(bool),
    /// A variable: a local, or a syntactic parameter by name (15.3/15.6:
    /// parameters are string variables).
    Var(String),
    /// `+` concatenation (both operands must be strings).
    Concat(Box<CompExpr>, Box<CompExpr>),
    /// `==` / `!=` on two same-kind values.
    Equal(Box<CompExpr>, Box<CompExpr>),
    NotEqual(Box<CompExpr>, Box<CompExpr>),
    /// A call; only `reject_match()` with no arguments is known.
    Call {
        name: String,
        args: Vec<CompExpr>,
    },
}

/// An evaluated value. Locals hold strings only; booleans exist for
/// conditions and comparisons.
#[derive(Debug, Clone, PartialEq, Eq)]
enum CompValue {
    Str(String),
    Bool(bool),
}

/// Why a definition declined. `Reject` is the standard's mechanism (a
/// `reject_match()` action executed); `Error` is any other evaluation
/// failure. Both decline the definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompFail {
    Reject,
    Error,
}

/// Control flow out of a statement block.
#[derive(Debug, Clone, PartialEq, Eq)]
enum CompFlow {
    Next,
    Return(String),
}

/// The text a matched subsequence renders to when a computed body reads
/// it as a string. Tokens are joined with single spaces: tokenization
/// ignores whitespace, so the text re-tokenizes to the same tokens, and
/// the spaces keep adjacent `-`/`/` punctuation from forming a comment.
/// String literals are re-quoted (their stored value keeps escapes).
fn render_tokens_text(tokens: &[Token]) -> String {
    let parts: Vec<String> = tokens
        .iter()
        .map(|token| match &token.kind {
            TokenKind::Punctuation(symbol) => symbol.clone(),
            TokenKind::Identifier(word) | TokenKind::Keyword(word) => word.clone(),
            TokenKind::Number(text) => text.clone(),
            TokenKind::StringLiteral(value) => format!("\"{value}\""),
        })
        .collect();
    parts.join(" ")
}

/// Evaluate a computed body over the matched parameters; `None` means
/// the definition declines. The body must parse wholly into the
/// restricted subset; on success the returned string is the expansion
/// text.
fn evaluate_computed_body(
    body: &[Token],
    parameters: &[(Option<String>, String, Vec<Token>)],
) -> Option<String> {
    let mut parser = CompParser {
        tokens: body,
        parameters,
        cursor: 0,
    };
    let stmts = parser.parse_statement_list(true).ok()?;
    if parser.cursor != body.len() {
        return None;
    }
    let mut env = HashMap::new();
    // Parameters as string variables: by tag always; by non-terminal
    // kind only when unique (15.6(a): otherwise the reference is
    // undefined). Locals later shadow them by plain assignment.
    for (tag, _, tokens) in parameters {
        if let Some(tag) = tag {
            env.insert(tag.clone(), render_tokens_text(tokens));
        }
    }
    for (_, kind, tokens) in parameters {
        if parameters
            .iter()
            .filter(|(parameter_tag, parameter_kind, _)| {
                parameter_tag.is_none() && parameter_kind == kind
            })
            .count()
            == 1
        {
            env.entry(kind.clone())
                .or_insert_with(|| render_tokens_text(tokens));
        }
    }
    let result = match exec_statements(&stmts, &mut env) {
        Ok(CompFlow::Return(text)) => Some(text),
        // Falling off the end: the `result` variable is the return value
        // (15.3). A body that assigned neither declines.
        Ok(CompFlow::Next) => env.get("result").cloned(),
        Err(_) => None,
    };
    // An empty string is not a legal construct of the category (15.3).
    result.filter(|text| !text.is_empty())
}

fn exec_statements(
    stmts: &[CompStmt],
    env: &mut HashMap<String, String>,
) -> Result<CompFlow, CompFail> {
    for stmt in stmts {
        match stmt {
            CompStmt::Assign { name, expr } => match eval_expr(expr, env)? {
                CompValue::Str(text) => {
                    env.insert(name.clone(), text);
                }
                CompValue::Bool(_) => return Err(CompFail::Error),
            },
            CompStmt::If {
                cond,
                then_body,
                else_body,
            } => match eval_expr(cond, env)? {
                CompValue::Bool(taken) => {
                    let branch: &[CompStmt] = if taken {
                        then_body
                    } else {
                        else_body.as_deref().unwrap_or(&[])
                    };
                    if let CompFlow::Return(text) = exec_statements(branch, env)? {
                        return Ok(CompFlow::Return(text));
                    }
                }
                CompValue::Str(_) => return Err(CompFail::Error),
            },
            CompStmt::Return(Some(expr)) => match eval_expr(expr, env)? {
                CompValue::Str(text) => return Ok(CompFlow::Return(text)),
                CompValue::Bool(_) => return Err(CompFail::Error),
            },
            CompStmt::Return(None) => return Err(CompFail::Error),
            CompStmt::Expr(expr) => {
                eval_expr(expr, env)?;
            }
        }
    }
    Ok(CompFlow::Next)
}

fn eval_expr(expr: &CompExpr, env: &HashMap<String, String>) -> Result<CompValue, CompFail> {
    match expr {
        CompExpr::Str(text) => Ok(CompValue::Str(text.clone())),
        CompExpr::Bool(value) => Ok(CompValue::Bool(*value)),
        CompExpr::Var(name) => env
            .get(name)
            .map(|text| CompValue::Str(text.clone()))
            .ok_or(CompFail::Error),
        CompExpr::Concat(left, right) => match (eval_expr(left, env)?, eval_expr(right, env)?) {
            (CompValue::Str(a), CompValue::Str(b)) => Ok(CompValue::Str(a + &b)),
            _ => Err(CompFail::Error),
        },
        CompExpr::Equal(left, right) | CompExpr::NotEqual(left, right) => {
            let negate = matches!(expr, CompExpr::NotEqual(..));
            let equal = match (eval_expr(left, env)?, eval_expr(right, env)?) {
                (CompValue::Str(a), CompValue::Str(b)) => a == b,
                (CompValue::Bool(a), CompValue::Bool(b)) => a == b,
                _ => return Err(CompFail::Error),
            };
            Ok(CompValue::Bool(equal != negate))
        }
        CompExpr::Call { name, args } => {
            // Arguments evaluate first, so a nested reject_match()
            // declines even inside an unknown call.
            for arg in args {
                eval_expr(arg, env)?;
            }
            if name == "reject_match" && args.is_empty() {
                // The standard's decline action: the definition does not
                // take this match.
                return Err(CompFail::Reject);
            }
            // Any other routine is unknown at parse time (15.3: module
            // definitions cannot be presupposed) — the evaluation fails
            // and the definition declines.
            Err(CompFail::Error)
        }
    }
}

/// Recursive-descent parser for the restricted computed-body subset
/// over the recorded body tokens. Any construct outside the subset is a
/// parse failure (the body then declines).
struct CompParser<'a> {
    tokens: &'a [Token],
    parameters: &'a [(Option<String>, String, Vec<Token>)],
    cursor: usize,
}

impl<'a> CompParser<'a> {
    fn kind_at(&self, index: usize) -> Option<&'a TokenKind> {
        self.tokens.get(index).map(|token| &token.kind)
    }

    fn at_punct(&self, symbol: &str) -> bool {
        matches!(self.kind_at(self.cursor), Some(TokenKind::Punctuation(p)) if p == symbol)
    }

    /// Consume the punctuation `symbol` when present.
    fn take_punct(&mut self, symbol: &str) -> bool {
        if self.at_punct(symbol) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn expect_punct(&mut self, symbol: &str) -> Result<(), ()> {
        if self.take_punct(symbol) {
            Ok(())
        } else {
            Err(())
        }
    }

    fn at_keyword(&self, word: &str) -> bool {
        matches!(self.kind_at(self.cursor), Some(TokenKind::Keyword(w)) if w == word)
    }

    /// The `;` terminating a simple statement. The body segment's
    /// trailing `;` is consumed by the definition scanner (15.4.6 — it
    /// terminates the body segment, not the expansion), so the last
    /// statement of the body may end without one.
    fn expect_statement_end(&mut self) -> Result<(), ()> {
        if self.take_punct(";") || self.cursor == self.tokens.len() {
            return Ok(());
        }
        Err(())
    }

    fn take_keyword(&mut self, word: &str) -> bool {
        if self.at_keyword(word) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    /// Statements until end of tokens (`top`) or the closing `}`.
    fn parse_statement_list(&mut self, top: bool) -> Result<Vec<CompStmt>, ()> {
        let mut stmts = Vec::new();
        loop {
            match self.kind_at(self.cursor) {
                None => {
                    if top {
                        return Ok(stmts);
                    }
                    return Err(()); // unterminated block
                }
                Some(TokenKind::Punctuation(p)) if p == "}" => {
                    if top {
                        return Err(()); // stray closing brace
                    }
                    return Ok(stmts);
                }
                Some(_) => stmts.push(self.parse_statement()?),
            }
        }
    }

    fn parse_statement(&mut self) -> Result<CompStmt, ()> {
        if self.take_keyword("var") {
            let name = self.parse_name()?;
            if !self.take_punct(":=") {
                if self.take_punct(":") {
                    // `var name: <type> = expr;` — the type is recorded
                    // shape only; skip to the initializer.
                    while !self.take_punct("=") {
                        self.kind_at(self.cursor).ok_or(())?;
                        self.cursor += 1;
                    }
                } else {
                    self.expect_punct("=")?;
                }
            }
            let expr = self.parse_expr()?;
            self.expect_statement_end()?;
            return Ok(CompStmt::Assign { name, expr });
        }
        if self.take_keyword("return") {
            if self.take_punct(";") {
                return Ok(CompStmt::Return(None));
            }
            let expr = self.parse_expr()?;
            self.expect_statement_end()?;
            return Ok(CompStmt::Return(Some(expr)));
        }
        if self.take_keyword("if") {
            let cond = self.parse_expr()?;
            self.expect_punct("{")?;
            let then_body = self.parse_statement_list(false)?;
            self.expect_punct("}")?;
            let mut else_body = None;
            if self.take_keyword("else") {
                self.expect_punct("{")?;
                let body = self.parse_statement_list(false)?;
                self.expect_punct("}")?;
                else_body = Some(body);
            }
            // The corpus style terminates an if with `};` — the
            // semicolon after the block is optional.
            self.take_punct(";");
            return Ok(CompStmt::If {
                cond,
                then_body,
                else_body,
            });
        }
        // A plain assignment `name = expr;` (the `==` comparison lexes
        // as one token, so a lone `=` is unambiguous).
        if let Some(TokenKind::Identifier(name)) = self.kind_at(self.cursor)
            && matches!(self.kind_at(self.cursor + 1), Some(TokenKind::Punctuation(p)) if p == "=")
        {
            let name = name.clone();
            self.cursor += 2;
            let expr = self.parse_expr()?;
            self.expect_statement_end()?;
            return Ok(CompStmt::Assign { name, expr });
        }
        let expr = self.parse_expr()?;
        self.expect_statement_end()?;
        Ok(CompStmt::Expr(expr))
    }

    /// A local-variable name: a plain identifier.
    fn parse_name(&mut self) -> Result<String, ()> {
        match self.kind_at(self.cursor) {
            Some(TokenKind::Identifier(word)) => {
                let name = word.clone();
                self.cursor += 1;
                Ok(name)
            }
            _ => Err(()),
        }
    }

    /// `expr := cmp [('=='|'!=') cmp]` — comparisons bind loosest.
    fn parse_expr(&mut self) -> Result<CompExpr, ()> {
        let left = self.parse_concat()?;
        if self.take_punct("==") {
            let right = self.parse_concat()?;
            return Ok(CompExpr::Equal(Box::new(left), Box::new(right)));
        }
        if self.take_punct("!=") {
            let right = self.parse_concat()?;
            return Ok(CompExpr::NotEqual(Box::new(left), Box::new(right)));
        }
        Ok(left)
    }

    /// `cmp := primary ('+' primary)*` — `+` concatenates strings.
    fn parse_concat(&mut self) -> Result<CompExpr, ()> {
        let mut left = self.parse_primary()?;
        while self.take_punct("+") {
            let right = self.parse_primary()?;
            left = CompExpr::Concat(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_primary(&mut self) -> Result<CompExpr, ()> {
        match self.kind_at(self.cursor) {
            Some(TokenKind::StringLiteral(value)) => {
                // The token value keeps the escape sequences raw; the
                // string's value interprets them (matching how e string
                // values read everywhere else).
                let text = crate::interp::unescape(value);
                self.cursor += 1;
                Ok(CompExpr::Str(text))
            }
            Some(TokenKind::Keyword(word)) if word == "TRUE" => {
                self.cursor += 1;
                Ok(CompExpr::Bool(true))
            }
            Some(TokenKind::Keyword(word)) if word == "FALSE" => {
                self.cursor += 1;
                Ok(CompExpr::Bool(false))
            }
            // A `<...>` parameter reference: the matched submatch's text
            // as a string (the same resolution rules the simple body's
            // splicing uses). An unresolvable reference fails the body.
            Some(TokenKind::Punctuation(p)) if p == "<" => {
                self.cursor += 1;
                let mut inner = Vec::new();
                let mut closed = false;
                while self.cursor < self.tokens.len() {
                    if matches!(self.kind_at(self.cursor), Some(TokenKind::Punctuation(p)) if p == ">")
                    {
                        closed = true;
                        break;
                    }
                    inner.push(&self.tokens[self.cursor]);
                    self.cursor += 1;
                }
                if !closed {
                    return Err(());
                }
                self.cursor += 1; // '>'
                let submatch = resolve_reference(&inner, self.parameters).ok_or(())?;
                Ok(CompExpr::Str(render_tokens_text(&submatch)))
            }
            Some(TokenKind::Punctuation(p)) if p == "(" => {
                self.cursor += 1;
                let expr = self.parse_expr()?;
                self.expect_punct(")")?;
                Ok(expr)
            }
            Some(TokenKind::Identifier(name)) => {
                let name = name.clone();
                self.cursor += 1;
                if self.take_punct("(") {
                    let mut args = Vec::new();
                    if !self.take_punct(")") {
                        loop {
                            args.push(self.parse_expr()?);
                            if self.take_punct(",") {
                                continue;
                            }
                            self.expect_punct(")")?;
                            break;
                        }
                    }
                    return Ok(CompExpr::Call { name, args });
                }
                Ok(CompExpr::Var(name))
            }
            _ => Err(()),
        }
    }
}
