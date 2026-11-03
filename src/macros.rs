//! The macro engine (clause 15): match-expression parsing (15.4) and
//! simple-macro expansion (15.5/15.6) as a pre-pass over the token
//! stream.
//!
//! A macro definition (`define <name'category> "pattern" as { body };`)
//! associates a new production with the non-terminal named by its
//! category. This module compiles the recorded pattern strings into a
//! term tree (15.4: escaped literal terminals, `<[tag']type>`
//! non-terminals, `[ ]` option, `( )` grouping, `|` alternation),
//! matches token sequences against them with backtracking (15.5.2's
//! recursive-descent reading), and expands a simple macro's body by
//! splicing the matched subsequence at the parameter references
//! (15.6: "the expansion segment is taken literally, except for
//! occurrences of syntactic parameters").
//!
//! The pass runs before the parser reads the token stream, so
//! `MIN_VALUE(addr_t)`-style invocation sites anywhere in ordinary
//! code become their body text and parse like any other code. Later
//! definitions have higher priority (15.5.1); the first macro whose
//! whole pattern matches wins. V1 approximations, documented: a
//! literal terminal matches one token whose whole text equals it
//! (whitespace-only literals vanish — tokenization ignores
//! whitespace, so the `[ ]` optional-space form is vacuous);
//! non-terminals consume by backtracking over split lengths; the
//! `<type, ...>` repetition form matches as one non-terminal; computed
//! macros (define-as-computed) are recorded, not computed — the
//! matcher skips them, so a file's simple definition of the same
//! pattern applies (the corpus's `reject_match()` dance is not
//! modeled); pattern-argument categories are not validated.

use crate::lexer::{Token, TokenKind};

/// One term of a compiled match expression (15.4).
#[derive(Debug, Clone, PartialEq, Eq)]
enum MatchTerm {
    /// A literal terminal: matches one token whose whole text equals
    /// it (15.4.1; whitespace-only literals are dropped at compile
    /// time).
    Literal(String),
    /// `<[tag']kind>` — a syntactic argument (15.4.1) that consumes a
    /// subsequence of tokens by backtracking.
    NonTerminal { tag: Option<String>, kind: String },
    /// `[ ... ]` (optional) or `( ... )` (grouping) with `|`
    /// alternatives (15.4.2); an option succeeds even when nothing
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
/// priority; computed definitions are skipped) and render the
/// expansion with the matched parameters. Returns the expansion and
/// the position after the matched span.
fn expand_at(
    macros: &[ScannedMacro],
    tokens: &[Token],
    position: usize,
) -> Option<(Expansion, usize)> {
    for scanned in macros.iter().rev() {
        if scanned.computed {
            continue;
        }
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
        if match_terms(
            &scanned.terms,
            tokens,
            &mut cursor,
            &mut parameters,
            &mut budget,
        ) {
            return Some((
                Expansion {
                    tokens: render_body(&scanned.body, &parameters),
                },
                cursor,
            ));
        }
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
    // (15.4.5: the semicolon's function is fixed to sequencing inside
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
