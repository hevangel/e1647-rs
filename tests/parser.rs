use e1647::ast::{BinaryOp, Declaration, DeclarationKind, Expr, Member, ScalarBase, TypeExpr};
use e1647::parser::parse;

fn expect_clean(source: &str) -> Vec<Declaration> {
    let result = parse(source);
    assert!(
        result.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
    result.declarations
}

#[test]
fn packet_example_parses() {
    let source = include_str!("../examples/packet.e");
    let declarations = expect_clean(source);
    assert_eq!(declarations.len(), 1);
    let declaration = &declarations[0];
    assert_eq!(declaration.kind, DeclarationKind::Struct);
    assert_eq!(declaration.name, "packet");
    assert_eq!(declaration.members.len(), 4);
    let Member::Field(addr) = &declaration.members[0] else {
        panic!("expected a field");
    };
    assert_eq!(addr.name, "addr");
    let TypeExpr::Scalar {
        base: ScalarBase::Uint,
        width: Some(width),
        ..
    } = &addr.ty
    else {
        panic!("expected a width-8 uint");
    };
    let Expr::IntLiteral { text, .. } = width.as_ref() else {
        panic!("expected an integer width");
    };
    assert_eq!(text, "8");
    let Member::Constraint(constraint) = &declaration.members[2] else {
        panic!("expected a keep constraint");
    };
    let Expr::Binary {
        op: BinaryOp::Lt, ..
    } = constraint.expr
    else {
        panic!("expected a '<' constraint");
    };
}

#[test]
fn extend_like_and_list_types() {
    let declarations = expect_clean(
        "struct frame {\n    payload: list of uint(bits: 16);\n    tag: nibble;\n};\n\
         extend frame {\n    keep tag > 0;\n};\n\
         struct extended_frame like frame {\n    crc: uint(bits: 32);\n};\n",
    );
    assert_eq!(declarations.len(), 3);
    assert_eq!(declarations[0].kind, DeclarationKind::Struct);
    let Member::Field(payload) = &declarations[0].members[0] else {
        panic!("expected a field");
    };
    let TypeExpr::List { element, .. } = &payload.ty else {
        panic!("expected a list type");
    };
    let TypeExpr::Scalar {
        base: ScalarBase::Uint,
        width: Some(width),
        ..
    } = element.as_ref()
    else {
        panic!("expected a width-16 uint list element");
    };
    let Expr::IntLiteral { text, .. } = width.as_ref() else {
        panic!("expected an integer width");
    };
    assert_eq!(text, "16");
    assert_eq!(declarations[1].kind, DeclarationKind::Extend);
    assert_eq!(declarations[1].name, "frame");
    assert_eq!(declarations[2].like.as_deref(), Some("frame"));
}

#[test]
fn expression_precedence() {
    let declarations = expect_clean(
        "struct s {\n    flag: bool;\n    a: uint;\n    b: uint;\n    x: uint;\n    \
         keep flag || a + b * 2 == 10 && !(x > 3);\n};\n",
    );
    let Member::Constraint(constraint) = &declarations[0].members[4] else {
        panic!("expected a keep constraint");
    };
    // `||` binds loosest in the C-style precedence, so it is at the top:
    // flag || ((a + b * 2 == 10) && !(x > 3))
    let Expr::Binary {
        op: BinaryOp::LogicalOr,
        lhs,
        rhs,
        ..
    } = &constraint.expr
    else {
        panic!("expected || at the top");
    };
    let Expr::Ident { name, .. } = lhs.as_ref() else {
        panic!("expected the bare flag on the left");
    };
    assert_eq!(name, "flag");
    let Expr::Binary {
        op: BinaryOp::LogicalAnd,
        ..
    } = rhs.as_ref()
    else {
        panic!("expected && under the ||");
    };
}

#[test]
fn literals_and_member_access() {
    let declarations = expect_clean(
        "struct cfg_t {\n    mode: uint;\n};\nstruct s {\n    cfg: cfg_t;\n    addr: uint;\n    \
         keep cfg.mode == 8'hFF;\n    keep addr == 0b1010;\n    keep -addr <= +1.5;\n};\n",
    );
    let Member::Constraint(constraint) = &declarations[1].members[2] else {
        panic!("expected a keep constraint");
    };
    let Expr::Binary {
        op: BinaryOp::Eq,
        lhs,
        rhs,
        ..
    } = &constraint.expr
    else {
        panic!("expected == constraint");
    };
    let Expr::MemberAccess { .. } = lhs.as_ref() else {
        panic!("expected member access");
    };
    let Expr::BitLiteral { text, .. } = rhs.as_ref() else {
        panic!("expected a based literal");
    };
    assert_eq!(text, "8'hFF");
}

#[test]
fn diagnostics_are_reported_with_positions() {
    let result = parse(
        "struct packet {\n    addr: uint(bits: 8);\n    addr: uint(bits: 8);\n    keep size < 4;\n};\n",
    );
    let rendered: Vec<String> = result.diagnostics.iter().map(|d| d.render()).collect();
    assert_eq!(
        rendered,
        vec![
            "3:5: error[E0202]: duplicate field 'addr'",
            "4:10: error[E0204]: unknown field 'size' in keep constraint",
        ]
    );
}

#[test]
fn lex_errors_become_diagnostics() {
    let result = parse("struct s { keep x < 'z1; };");
    assert_eq!(result.declarations.len(), 0);
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].code, "E0001");
}

#[test]
fn recovery_reports_each_bad_declaration() {
    let result = parse("foo bar;\nstruct ok_s { a: uint; };\nunit also_bad;\n");
    let codes: Vec<&str> = result.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes.contains(&"E0101"), "codes: {codes:?}");
    assert!(codes.contains(&"E0106"), "codes: {codes:?}");
    // Declarations are kept even when the body failed to parse; the empty
    // `also_bad` skeleton stands in for the recovery point and the
    // diagnostics are authoritative.
    assert_eq!(result.declarations.len(), 2);
    assert_eq!(result.declarations[0].name, "ok_s");
    assert_eq!(result.declarations[0].members.len(), 1);
    assert_eq!(result.declarations[1].name, "also_bad");
    assert!(result.declarations[1].members.is_empty());
}

#[test]
fn unsupported_members_report_explicitly() {
    let result = parse("struct s {\n    event done;\n    a: uint;\n};\n");
    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec!["E0113"]
    );
    // The parser recovers and keeps the well-formed field.
    assert_eq!(result.declarations[0].members.len(), 1);
}

#[test]
fn soft_constraints_are_rejected_for_now() {
    let result = parse("struct s { a: uint; keep soft a < 10; };\n");
    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec!["E0114"]
    );
}

#[test]
fn field_name_groups_are_rejected_for_now() {
    let result = parse("struct s { a, b: uint; };\n");
    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec!["E0114"]
    );
}
