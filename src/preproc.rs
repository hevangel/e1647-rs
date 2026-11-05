//! The clause 21 preprocessor replacement rules, as a token-level
//! pre-pass over the stream (mirroring `src/macros.rs`'s macro
//! pre-pass). The parser's `#ifdef`/`#ifndef` flag handling stays in
//! `src/parser.rs`; this module applies the other half of `#define`.
//!
//! Clause 21.3: a defined name is both a flag (consulted by the
//! conditional directives) and a replacement rule — "the given text is
//! substituted for every occurrence of the name in subsequent code,
//! except inside string literals". This pass walks the token stream in
//! source order, keeps the working set of rules defined so far, and
//! splices each defined name's replacement tokens in place of the
//! identifier token. The parser then sees ordinary code.
//!
//! What the clause 21 prose confirms (the local redline PDF renders the
//! clause 21 syntax examples as images; only the prose was extractable):
//! - the replacement is the text between the name and the terminating
//!   `;` or the end of the name's line (the 21.3 example shows
//!   `#define PLIST_SIZE 100`); a replacement may cross a line break
//!   only behind a backslash, which the lexer never produces, so one
//!   line is the whole span — the same span
//!   `Parser::consume_hash_replacement` consumes;
//! - substitution reaches "every occurrence of the name in subsequent
//!   code, except inside string literals" — string literals are single
//!   tokens here and this pass rewrites only identifier tokens, so they
//!   are skipped by construction;
//! - "a #define statement applies only to code that comes later", and
//!   `#undef` removes the rule "from that point onward" (21.4);
//! - excluded `#ifdef`/`#ifndef` code is ignored (21.3), so a `#define`
//!   inside an excluded branch defines nothing and its region gets no
//!   substitutions.
//!
//! Documented v1 approximations:
//! - substitution is single-pass: the spliced replacement tokens are
//!   not rescanned, so a name that only appears inside another rule's
//!   replacement stays as written (and 21.3's "the replacement shall
//!   not contain the name" runtime error cannot loop here; it is not
//!   diagnosed);
//! - redefining a name without an intervening `#undef` takes the last
//!   value, like an undefined-then-redefined rule (21.4);
//! - the backtick-prefixed Verilog-style preprocessor names of 4.1.5.4
//!   are not accepted — a rule's name must be a plain identifier token;
//! - cross-file visibility (Annex B.5's include order through
//!   `import`) is not modeled: each file is preprocessed independently;
//! - the `#else` group of a conditional whose first group is included
//!   is dead code and passes through unsubstituted, exactly like the
//!   parser's skipped branch.

use crate::lexer::{Token, TokenKind};
use std::collections::BTreeMap;

/// Substitute the clause 21.3 replacement rules through one token
/// stream, in source order.
pub fn substitute_defines(tokens: Vec<Token>) -> Vec<Token> {
    let mut rules: BTreeMap<String, Vec<Token>> = BTreeMap::new();
    let mut output = Vec::with_capacity(tokens.len());
    walk(&tokens, 0, tokens.len(), &mut rules, &mut output);
    output
}

/// Walk `tokens[start..end]`, splicing defined names' replacements and
/// recursing into included conditional groups. `rules` is shared across
/// the whole file: a `#define` stays in force after its enclosing brace
/// group ends (21.3 applies to code that comes later, not to a brace
/// scope).
fn walk(
    tokens: &[Token],
    start: usize,
    end: usize,
    rules: &mut BTreeMap<String, Vec<Token>>,
    output: &mut Vec<Token>,
) {
    let mut index = start;
    while index < end {
        // `directive` emits nothing and mutates no rule when it returns
        // `None`, so the short-circuit below is side-effect free.
        if is_punct(&tokens[index], "#")
            && let Some(next) = directive(tokens, index, end, rules, output)
        {
            index = next;
            continue;
        }
        match &tokens[index].kind {
            TokenKind::Identifier(word) => match rules.get(word) {
                Some(replacement) => output.extend(replacement.iter().cloned()),
                None => output.push(tokens[index].clone()),
            },
            _ => output.push(tokens[index].clone()),
        }
        index += 1;
    }
}

/// One scanned conditional directive: the primary group and the
/// optional `#else` group, as token-index ranges (the header is
/// `index..open`, the primary contents `open+1..close`).
struct ScannedConditional {
    open: usize,
    close: usize,
    /// `(semicolon, open, close)` of the `#else` part when present:
    /// the optional `;` between the groups, and the alternative's `{`
    /// and matching `}`.
    else_part: Option<(usize, usize, usize)>,
}

/// Process one `#` directive at `index` (a `#` punctuation token).
/// Returns the index after the directive's own span when the shape is a
/// well-formed `#define`/`#undef`/`#ifdef`/`#ifndef`, having emitted the
/// directive tokens and — for conditionals — the included group's
/// substituted contents; returns `None` otherwise, leaving the `#` to
/// flow through as an ordinary token (the parser diagnoses it).
///
/// The directive tokens themselves are always emitted verbatim: a rule
/// must not rewrite the directives that define or test it. The whole
/// conditional is scanned before anything is emitted, so a malformed
/// shape emits nothing.
fn directive(
    tokens: &[Token],
    index: usize,
    end: usize,
    rules: &mut BTreeMap<String, Vec<Token>>,
    output: &mut Vec<Token>,
) -> Option<usize> {
    let word = match tokens.get(index + 1)?.kind {
        TokenKind::Keyword(ref w) if w == "define" => "define",
        TokenKind::Identifier(ref w) if w == "ifdef" || w == "ifndef" || w == "undef" => w.as_str(),
        _ => return None,
    };
    let name_token = tokens.get(index + 2)?;
    let TokenKind::Identifier(name) = &name_token.kind else {
        return None;
    };
    match word {
        "define" => {
            // The replacement reaches the `;` or the end of the name's
            // line, mirroring `Parser::consume_hash_replacement`. The
            // terminating `;` itself is not replacement content and
            // flows through as an ordinary token.
            let name_line = name_token.start.line;
            let mut stop = index + 3;
            while stop < end && tokens[stop].start.line <= name_line {
                if is_punct(&tokens[stop], ";") {
                    break;
                }
                stop += 1;
            }
            rules.insert(name.clone(), tokens[index + 3..stop].to_vec());
            output.extend(tokens[index..stop].iter().cloned());
            Some(stop)
        }
        "undef" => {
            rules.remove(name);
            output.extend(tokens[index..index + 3].iter().cloned());
            Some(index + 3)
        }
        _ => {
            // `#ifdef` / `#ifndef`: evaluate the condition with the
            // rules in force here, then walk the included group and
            // carry the excluded one through untouched (21.3's "ignore
            // it").
            let included = if word == "ifdef" {
                rules.contains_key(name)
            } else {
                !rules.contains_key(name)
            };
            let scanned = scan_conditional(tokens, index, end)?;
            output.extend(tokens[index..=scanned.open].iter().cloned());
            if included {
                walk(tokens, scanned.open + 1, scanned.close, rules, output);
            } else {
                output.extend(tokens[scanned.open + 1..scanned.close].iter().cloned());
            }
            output.push(tokens[scanned.close].clone());
            if let Some((semicolon, else_open, else_close)) = scanned.else_part {
                // The `;` (when the `}; #else` spelling is used), the
                // `#else` header and its `{` all pass through verbatim;
                // only the winning group's contents are substituted.
                output.extend(tokens[semicolon..=else_open].iter().cloned());
                if included {
                    output.extend(tokens[else_open + 1..else_close].iter().cloned());
                } else {
                    walk(tokens, else_open + 1, else_close, rules, output);
                }
                output.push(tokens[else_close].clone());
                return Some(else_close + 1);
            }
            Some(scanned.close + 1)
        }
    }
}

/// Scan one `#ifdef`/`#ifndef` structure: the primary brace group and
/// the optional `#else` group after it — directly following the first
/// `}` or after the group's `;` (the parser accepts both spellings,
/// since the extractable prose does not fix the terminator's position).
/// `None` on any malformed shape; the caller then lets the `#` pass
/// through and the parser reports it.
fn scan_conditional(tokens: &[Token], index: usize, end: usize) -> Option<ScannedConditional> {
    let open = index + 3;
    if !is_open_brace(tokens.get(open)?) {
        return None;
    }
    let close = scan_group(tokens, open, end)?;
    // An optional `;` sits at `close + 1` (the `}; #else` spelling);
    // without it, `close + 1` is where a direct `#else` would start.
    let mut after = close + 1;
    if tokens.get(after).is_some_and(|token| is_punct(token, ";")) {
        after += 1;
    }
    let else_part =
        if at_else(tokens, after, end) && tokens.get(after + 2).is_some_and(is_open_brace) {
            let else_close = scan_group(tokens, after + 2, end)?;
            Some((close + 1, after + 2, else_close))
        } else {
            None
        };
    Some(ScannedConditional {
        open,
        close,
        else_part,
    })
}

/// Whether an `#else` directive starts at `index` — the same shape the
/// parser accepts (`else` lexes as a keyword; the identifier form is
/// tolerated for symmetry with the other directive words).
fn at_else(tokens: &[Token], index: usize, end: usize) -> bool {
    index + 1 < end
        && is_punct(&tokens[index], "#")
        && matches!(
            &tokens[index + 1].kind,
            TokenKind::Keyword(w) | TokenKind::Identifier(w) if w == "else"
        )
}

fn is_open_brace(token: &Token) -> bool {
    is_punct(token, "{")
}

fn is_punct(token: &Token, symbol: &str) -> bool {
    matches!(&token.kind, TokenKind::Punctuation(p) if p == symbol)
}

/// The index of the `}` matching the `{` at `open`, counting nested
/// braces (the tokens inside are counted, not parsed — 21.3's ignored
/// code). `None` when the group never closes within `end`.
fn scan_group(tokens: &[Token], open: usize, end: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut index = open;
    while index < end {
        match &tokens[index].kind {
            TokenKind::Punctuation(p) if p == "{" => depth += 1,
            TokenKind::Punctuation(p) if p == "}" => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}
