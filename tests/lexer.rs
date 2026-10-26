use e1647::lexer::{KEYWORDS, TokenKind, is_keyword, tokenize};
use std::collections::BTreeSet;

/// The single-token keyword spellings of the reference grammar
/// (`external/specman-e-parser/specman.flex`, MIT licensed), transcribed for
/// this cross-check. Deviations in `KEYWORDS`, as documented there: the
/// IEEE 1647 predefined types `real` and `string` are added; the multi-word
/// reference keywords (`is a`, `is not a`, `state machine`, `C export`,
/// `C routine`) are not single-token keywords.
const REFERENCE_KEYWORDS: &[&str] = &[
    "while",
    "repeat",
    "until",
    "for",
    "reverse",
    "in",
    "from",
    "down",
    "do",
    "to",
    "line",
    "file",
    "matching",
    "break",
    "continue",
    "with",
    "when",
    "struct",
    "unit",
    "enclosing",
    "using",
    "each",
    "prev",
    "index",
    "within",
    "gen",
    "before",
    "not",
    "range",
    "empty",
    "now",
    "or",
    "and",
    "nxor",
    "xor",
    "nor",
    "nand",
    "all",
    "first",
    "of",
    "me",
    "it",
    "keeping",
    "keep",
    "like",
    "extend",
    "bind",
    "simple",
    "port",
    "method",
    "list",
    "bits",
    "bytes",
    "type",
    "cycle",
    "detach",
    "true",
    "false",
    "rise",
    "fall",
    "change",
    "delay",
    "consume",
    "fail",
    "eventually",
    "start",
    "compute",
    "return",
    "print",
    "emit",
    "wait",
    "sync",
    "only",
    "also",
    "undefined",
    "event",
    "package",
    "private",
    "protected",
    "sequence",
    "that",
    "check",
    "assume",
    "expect",
    "assert",
    "step",
    "new",
    "as_a",
    "as_computed",
    "soft",
    "import",
    "key",
    "on",
    "exec",
    "if",
    "then",
    "else",
    "try",
    "case",
    "default",
    "is",
    "cover",
    "global",
    "no_collect",
    "per_unit_instance",
    "e_path",
    "radix",
    "dec",
    "hex",
    "bin",
    "text",
    "weight",
    "at_least",
    "ignore",
    "illegal",
    "no_trace",
    "num_of_buckets",
    "per_instance",
    "cross",
    "transition",
    "int",
    "uint",
    "bit",
    "byte",
    "nibble",
    "time",
    "bool",
    "var",
    "force",
    "release",
    "instance",
    "item",
    "state_machine",
    "routine",
    "c_export",
    "c_routine",
    "define",
    "as",
    "select",
    "TRUE",
    "FALSE",
    "NULL",
    "real",
    "string",
];

fn kinds(source: &str) -> Vec<TokenKind> {
    tokenize(source)
        .expect("valid source")
        .into_iter()
        .map(|token| token.kind)
        .collect()
}

#[test]
fn keywords_match_reference_grammar_set() {
    let declared: BTreeSet<&str> = KEYWORDS.iter().copied().collect();
    let reference: BTreeSet<&str> = REFERENCE_KEYWORDS.iter().copied().collect();
    let extra: Vec<&str> = declared.difference(&reference).copied().collect();
    let missing: Vec<&str> = reference.difference(&declared).copied().collect();
    assert!(
        extra.is_empty() && missing.is_empty(),
        "KEYWORDS drifted from the transcribed reference grammar set; extra: {extra:?}, missing: {missing:?}. \
         Update both lists deliberately and record the reason."
    );
}

#[test]
fn every_declared_keyword_lexes_as_keyword() {
    for keyword in KEYWORDS {
        let tokenized = tokenize(keyword).expect("keyword is valid source");
        assert_eq!(
            tokenized.len(),
            1,
            "'{keyword}' must be one token, got {tokenized:?}"
        );
        assert_eq!(
            tokenized[0].kind,
            TokenKind::Keyword((*keyword).to_owned()),
            "'{keyword}' did not lex as a keyword"
        );
        assert!(is_keyword(keyword), "is_keyword disagrees with KEYWORDS");
    }
}

#[test]
fn packet_example_tokenizes() {
    let source = include_str!("../examples/packet.e");
    let tokens = tokenize(source).expect("valid input");
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenKind::Keyword("struct".into()))
    );
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenKind::Keyword("keep".into()))
    );
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenKind::Number("128".into()))
    );
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenKind::Identifier("addr".into()))
    );
}

#[test]
fn keywords_are_recognized_exactly() {
    let parsed = kinds("struct unit extend keep like list of bits uint int bool string TRUE NULL");
    assert!(parsed.iter().all(|k| matches!(k, TokenKind::Keyword(_))));
    // Exact match only: a keyword prefix is a plain identifier.
    assert_eq!(
        kinds("structs keeping"),
        vec![
            TokenKind::Identifier("structs".into()),
            TokenKind::Keyword("keeping".into())
        ]
    );
}

#[test]
fn operators_longest_match() {
    let kinds = kinds("=== !== == != <= >= => && || << >> .. < > ! ^ & | ~ + - * / % ? @ $");
    assert_eq!(kinds.len(), 27);
    assert!(kinds.contains(&TokenKind::Punctuation("===".into())));
    assert!(kinds.contains(&TokenKind::Punctuation("!==".into())));
    assert!(kinds.contains(&TokenKind::Punctuation("=>".into())));
    assert!(kinds.contains(&TokenKind::Punctuation("..".into())));
    assert!(kinds.contains(&TokenKind::Punctuation("$".into())));
}

#[test]
fn based_and_prefixed_numerals() {
    assert_eq!(
        kinds("8'b1010 'hFF 'b1010 'd15 'o17 'x1f 'F0F 0xAB 0b1010 0o17 0xAB CD"),
        vec![
            TokenKind::Number("8'b1010".into()),
            TokenKind::Number("'hFF".into()),
            TokenKind::Number("'b1010".into()),
            TokenKind::Number("'d15".into()),
            TokenKind::Number("'o17".into()),
            TokenKind::Number("'x1f".into()),
            TokenKind::Number("'F0F".into()),
            TokenKind::Number("0xAB".into()),
            TokenKind::Number("0b1010".into()),
            TokenKind::Number("0o17".into()),
            // A based literal may be followed by an identifier that starts
            // with base-letter-shaped text.
            TokenKind::Number("0xAB".into()),
            TokenKind::Identifier("CD".into()),
        ]
    );
}

#[test]
fn reals_and_ranges() {
    assert_eq!(
        kinds("1.5 1e3 1.5e-2 7E2 1..5"),
        vec![
            TokenKind::Number("1.5".into()),
            TokenKind::Number("1e3".into()),
            TokenKind::Number("1.5e-2".into()),
            TokenKind::Number("7E2".into()),
            TokenKind::Number("1".into()),
            TokenKind::Punctuation("..".into()),
            TokenKind::Number("5".into()),
        ]
    );
}

#[test]
fn rejects_bad_based_literals() {
    let error = tokenize("''").unwrap_err();
    assert_eq!(error.message, "character literals are not supported yet");
    let error = tokenize("8'xZZ").unwrap_err();
    assert_eq!(error.message, "invalid based literal");
    let error = tokenize("'d").unwrap_err();
    assert_eq!(error.message, "invalid based literal");
    let error = tokenize("8'").unwrap_err();
    assert_eq!(error.message, "invalid based literal");
}

#[test]
fn subtype_modifier_quote_tokenizes_standalone() {
    assert_eq!(
        kinds("BUSY'kind"),
        vec![
            TokenKind::Identifier("BUSY".into()),
            TokenKind::Punctuation("'".into()),
            TokenKind::Identifier("kind".into()),
        ]
    );
    // A base letter right after `'` still starts a based literal.
    assert_eq!(kinds("'hFF"), vec![TokenKind::Number("'hFF".into())]);
}

#[test]
fn rejects_code_literal_delimiters() {
    let error = tokenize("<' extend foo '>").unwrap_err();
    assert_eq!(error.message, "code literals (<') are not supported yet");
}

#[test]
fn comments_and_source_locations() {
    let tokens = tokenize("// hi\nfoo /* bar */ <= 7").unwrap();
    assert_eq!(tokens.len(), 3);
    assert_eq!(tokens[0].start.line, 2);
    assert_eq!(tokens[0].start.column, 1);
    assert_eq!(tokens[1].kind, TokenKind::Punctuation("<=".into()));
}

#[test]
fn reports_unterminated_comment() {
    let error = tokenize("foo /* not closed").unwrap_err();
    assert_eq!(error.message, "unterminated block comment");
}

#[test]
fn reports_unterminated_string() {
    let error = tokenize("name: string = \"abc").unwrap_err();
    assert_eq!(error.message, "unterminated string literal");
}
