use e1647::lexer::{TokenKind, tokenize};

#[test]
fn packet_example_tokenizes() {
    let source = include_str!("../examples/packet.e");
    let tokens = tokenize(source).expect("valid input");
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenKind::Identifier("struct".into()))
    );
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenKind::Identifier("keep".into()))
    );
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenKind::Number("128".into()))
    );
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
