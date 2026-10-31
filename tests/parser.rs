use e1647::ast::{
    BinaryOp, Declaration, DeclarationKind, Expr, Member, MethodModifier, ScalarBase, Stmt,
    TypeExpr,
};
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
fn type_declarations_and_enumerals_parse() {
    let declarations = expect_clean(
        "type color_e: [RED, GREEN, BLUE];\n\
         struct pixel {\n    color: color_e;\n    keep color == GREEN;\n};\n",
    );
    assert_eq!(declarations.len(), 2);
    assert_eq!(declarations[0].kind, DeclarationKind::Type);
    assert_eq!(declarations[0].name, "color_e");
    assert_eq!(declarations[0].members.len(), 3);
    let Member::Enumeral(red) = &declarations[0].members[0] else {
        panic!("expected an enumeral");
    };
    assert_eq!(red.name, "RED");
    assert!(red.value.is_none());
    let Member::Enumeral(green) = &declarations[0].members[1] else {
        panic!("expected an enumeral");
    };
    assert_eq!(green.name, "GREEN");
    let Member::Field(color) = &declarations[1].members[0] else {
        panic!("expected a field");
    };
    let TypeExpr::Named { name, .. } = &color.ty else {
        panic!("expected a named type");
    };
    assert_eq!(name, "color_e");
}

#[test]
fn enumeral_explicit_values_parse() {
    let declarations = expect_clean("type opcode_e: [NOP = 0, ADD = 1];\n");
    let Member::Enumeral(add) = &declarations[0].members[1] else {
        panic!("expected an enumeral");
    };
    assert_eq!(add.name, "ADD");
    let Some(Expr::IntLiteral { text, .. }) = &add.value else {
        panic!("expected an explicit value");
    };
    assert_eq!(text, "1");
}

#[test]
fn methods_parse_with_params_return_and_statements() {
    let declarations = expect_clean(
        "struct packet_s {\n    len: uint;\n    get_len(): uint is {\n        return len;\n    };\n};\n\
         extend packet_s {\n    run() is also {\n        var p: uint = 3;\n        if p > 1 {\n            p = p - 1;\n        } else {\n            p = 0;\n        };\n        out(p);\n    };\n};\n",
    );
    assert_eq!(declarations.len(), 2);
    let Member::Method(get_len) = &declarations[0].members[1] else {
        panic!("expected a method");
    };
    assert_eq!(get_len.name, "get_len");
    assert!(get_len.params.is_empty());
    assert!(get_len.modifier.is_none());
    let Some(TypeExpr::Scalar {
        base: ScalarBase::Uint,
        width: None,
        ..
    }) = &get_len.return_type
    else {
        panic!("expected a plain uint return type");
    };
    assert_eq!(get_len.body.len(), 1);
    assert!(matches!(
        &get_len.body[0],
        Stmt::Return { value: Some(_), .. }
    ));
    let Member::Method(run) = &declarations[1].members[0] else {
        panic!("expected a method");
    };
    assert_eq!(run.name, "run");
    assert_eq!(run.modifier, Some(MethodModifier::Also));
    assert!(run.return_type.is_none());
    assert_eq!(run.body.len(), 3);
    assert!(matches!(&run.body[0], Stmt::VarDecl { init: Some(_), .. }));
    assert!(matches!(
        &run.body[1],
        Stmt::If {
            else_body: Some(_),
            ..
        }
    ));
    assert!(matches!(&run.body[2], Stmt::ExprStmt { .. }));
}

#[test]
fn method_calls_params_and_indexing_parse() {
    let declarations = expect_clean(
        "struct s {\n    data: list of uint;\n    sum(a: uint, b: uint): uint is {\n        var t: uint = a + b;\n        return t;\n    };\n    total(): uint is {\n        return self.sum(self.sum(1, 2), data[0]);\n    };\n};\n",
    );
    let Member::Method(sum) = &declarations[0].members[1] else {
        panic!("expected a method");
    };
    assert_eq!(sum.params.len(), 2);
    assert_eq!(sum.params[0].name, "a");
    assert_eq!(sum.params[1].name, "b");
    let Member::Method(total) = &declarations[0].members[2] else {
        panic!("expected a method");
    };
    let Some(Stmt::Return {
        value: Some(value), ..
    }) = total.body.first()
    else {
        panic!("expected a return statement");
    };
    let Expr::Call { callee, args, .. } = value else {
        panic!("expected a call expression");
    };
    let Expr::MemberAccess { name, .. } = callee.as_ref() else {
        panic!("expected a member-access callee");
    };
    assert_eq!(name, "sum");
    assert_eq!(args.len(), 2);
    let Expr::Call { args: nested, .. } = &args[0] else {
        panic!("expected a nested call");
    };
    assert_eq!(nested.len(), 2);
    let Expr::Index { .. } = &args[1] else {
        panic!("expected an index expression");
    };
}

#[test]
fn gen_and_print_statements_parse() {
    let declarations = expect_clean(
        "struct s {\n    p: uint;\n    run() is {\n        gen p;\n        print p, p + 1;\n    };\n};\n",
    );
    let Member::Method(run) = &declarations[0].members[1] else {
        panic!("expected a method");
    };
    assert_eq!(run.body.len(), 2);
    assert!(matches!(&run.body[0], Stmt::Gen { .. }));
    let Some(Stmt::Print { exprs, .. }) = run.body.get(1) else {
        panic!("expected a print statement");
    };
    assert_eq!(exprs.len(), 2);
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
    // `||` is looser than `&&`, which is looser than `==` (clause 4.5):
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
fn clause_4_5_places_xor_identity_and_in() {
    let declarations = expect_clean(
        "struct s {\n    a: uint;\n    b: uint;\n    c: uint;\n    flag: bool;\n    y: bool;\n    \
         keep a === b == c;\n    keep a == b === c;\n    keep flag && a in [1, 2];\n    \
         keep a in [1, 2] => y;\n    keep a | b ^ c;\n    keep a ^ b | c;\n};\n",
    );
    let keep = |index: usize| match &declarations[0].members[5 + index] {
        Member::Constraint(constraint) => &constraint.expr,
        _ => panic!("expected a keep constraint"),
    };
    // `==` is tighter than `===`: a === (b == c)
    let Expr::Binary {
        op: BinaryOp::IdentityEq,
        rhs,
        ..
    } = keep(0)
    else {
        panic!("expected === at the top of a === b == c");
    };
    assert!(matches!(
        rhs.as_ref(),
        Expr::Binary {
            op: BinaryOp::Eq,
            ..
        }
    ));
    // (a == b) === c
    let Expr::Binary {
        op: BinaryOp::IdentityEq,
        lhs,
        ..
    } = keep(1)
    else {
        panic!("expected === at the top of a == b === c");
    };
    assert!(matches!(
        lhs.as_ref(),
        Expr::Binary {
            op: BinaryOp::Eq,
            ..
        }
    ));
    // `in` is tighter than `&&`: flag && (a in [1, 2])
    let Expr::Binary {
        op: BinaryOp::LogicalAnd,
        rhs,
        ..
    } = keep(2)
    else {
        panic!("expected && at the top");
    };
    assert!(matches!(rhs.as_ref(), Expr::InSet { .. }));
    // `in` is tighter than `=>`: (a in [1, 2]) => y
    let Expr::Binary {
        op: BinaryOp::Imply,
        lhs,
        ..
    } = keep(3)
    else {
        panic!("expected => at the top");
    };
    assert!(matches!(lhs.as_ref(), Expr::InSet { .. }));
    // `|` is tighter than `^`: (a | b) ^ c
    let Expr::Binary {
        op: BinaryOp::BitXor,
        lhs,
        ..
    } = keep(4)
    else {
        panic!("expected ^ at the top of a | b ^ c");
    };
    assert!(matches!(
        lhs.as_ref(),
        Expr::Binary {
            op: BinaryOp::BitOr,
            ..
        }
    ));
    // a ^ (b | c)
    let Expr::Binary {
        op: BinaryOp::BitXor,
        rhs,
        ..
    } = keep(5)
    else {
        panic!("expected ^ at the top of a ^ b | c");
    };
    assert!(matches!(
        rhs.as_ref(),
        Expr::Binary {
            op: BinaryOp::BitOr,
            ..
        }
    ));
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
fn soft_constraints_parse_and_flag() {
    let declarations = expect_clean("struct s { a: uint; keep soft a < 10; keep a > 0; };\n");
    let Member::Constraint(soft) = &declarations[0].members[1] else {
        panic!("expected a keep constraint");
    };
    assert!(soft.is_soft);
    let Member::Constraint(hard) = &declarations[0].members[2] else {
        panic!("expected a keep constraint");
    };
    assert!(!hard.is_soft);
}

#[test]
fn field_name_groups_declare_one_field_per_name() {
    let declarations = expect_clean("struct s { a, b: uint; c: bool; };\n");
    let fields: Vec<&str> = declarations[0]
        .members
        .iter()
        .filter_map(|member| match member {
            Member::Field(field) => Some(field.name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(fields, vec!["a", "b", "c"]);
    let Member::Field(b) = &declarations[0].members[1] else {
        panic!("expected a field");
    };
    let TypeExpr::Scalar {
        base: ScalarBase::Uint,
        ..
    } = &b.ty
    else {
        panic!("expected the group type on every name");
    };
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
            "4:10: error[E0204]: unknown identifier 'size' in keep constraint",
        ]
    );
}

#[test]
fn lex_errors_become_diagnostics() {
    let result = parse("struct s { keep x < 8'b2; };");
    assert_eq!(result.declarations.len(), 0);
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].code, "E0001");
}

#[test]
fn quoted_hdl_paths_parse() {
    let declarations = expect_clean(
        "struct s {\n    run() is {\n        'memory_tb.address' = 8'hFF;\n        var v: uint = 'memory_tb.data_out';\n    };\n};\n",
    );
    let Member::Method(run) = &declarations[0].members[0] else {
        panic!("expected a method");
    };
    assert_eq!(run.body.len(), 2);
    assert!(
        matches!(
            &run.body[0],
            Stmt::Assign {
                target: Expr::HdlPath { path, .. },
                ..
            } if path == "memory_tb.address"
        ),
        "expected an HdlPath assignment target"
    );
    assert!(matches!(
        &run.body[1],
        Stmt::VarDecl {
            init: Some(Expr::HdlPath { path, .. }),
            ..
        } if path == "memory_tb.data_out"
    ));
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
    // `cover` members are supported now (clause 14); `assert` struct
    // members (clause 16) are the remaining unsupported family this
    // diagnostic pins.
    let result = parse("struct s {\n    b: uint;\n    assert never_zero: b != 0;\n};\n");
    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec!["E0113"]
    );
    // The parser recovers; the well-formed field survives in the partial
    // AST and the diagnostics are authoritative.
    assert_eq!(result.declarations[0].members.len(), 1);
}

#[test]
fn cover_groups_parse_with_items_options_and_extensions() {
    let declarations = expect_clean(
        "extend sys {\n    ev: event;\n    \
         cover ev is {\n            per_instance = TRUE;\n            \
         item a: uint (bits: 8) = sys.x using ranges = {\n                \
         range([0..10], \"low\");\n            };\n            \
         item b: bool = sys.y;;\n            cross a, b;\n            \
         transition a;\n            item c using also per_instance;\n            \
         item plain;\n            weight = 2;\n        };\n    \
         cover ev is also {\n            item d;\n        };\n};\n",
    );
    let extend = &declarations[0];
    let Member::Cover(group) = &extend.members[1] else {
        panic!("expected a cover group");
    };
    assert_eq!(group.name, "ev");
    // Four real items (a, b, c, plain) plus a cross and a transition;
    // the stray `;;` and the group option entries are tolerated.
    let item_kinds = group.items.iter().map(|item| item.kind).collect::<Vec<_>>();
    assert_eq!(
        item_kinds,
        vec![
            e1647::ast::CoverItemKind::Item,
            e1647::ast::CoverItemKind::Item,
            e1647::ast::CoverItemKind::Cross,
            e1647::ast::CoverItemKind::Transition,
            e1647::ast::CoverItemKind::Item,
            e1647::ast::CoverItemKind::Item,
        ]
    );
    // The ranges option block is captured verbatim, inner semicolons
    // included.
    let with_ranges = group
        .items
        .iter()
        .find(|item| item.ty.is_some() && !item.using.is_empty())
        .expect("expected the ranged item");
    let using_texts: Vec<String> = with_ranges
        .using
        .iter()
        .map(|token| match &token.kind {
            e1647::lexer::TokenKind::Identifier(word) => word.clone(),
            e1647::lexer::TokenKind::Keyword(word) => word.clone(),
            e1647::lexer::TokenKind::Punctuation(p) => p.clone(),
            _ => "?".to_owned(),
        })
        .collect();
    assert!(
        using_texts.iter().any(|text| text == "range"),
        "using tokens: {using_texts:?}"
    );
    // The `is also` extension form records its marker.
    let Member::Cover(extended) = &extend.members[2] else {
        panic!("expected the extended cover group");
    };
    assert!(extended.is_also);
    // Group-level option entries are recorded raw.
    assert!(!group.options.is_empty());
}

#[test]
fn typed_for_each_and_file_types_parse() {
    let declarations = expect_clean(
        "extend sys {\n    walk(fs: list of file) is {\n        \
         var f: file;\n        for each rf_struct (rs) in fs {\n            \
         out(rs);\n        };\n    };\n};\n",
    );
    let Member::Method(walk) = &declarations[0].members[0] else {
        panic!("expected a method");
    };
    let Some(Stmt::ForEach { type_filter, .. }) =
        walk.body.iter().find(|s| matches!(s, Stmt::ForEach { .. }))
    else {
        panic!("expected a for-each");
    };
    assert_eq!(type_filter.as_deref(), Some("rf_struct"));
}

#[test]
fn unsupported_statements_report_explicitly() {
    let result = parse(
        "struct s {\n    run() is {\n        repeat {\n            out(1);\n        };\n    };\n};\n",
    );
    let codes: Vec<&str> = result
        .diagnostics
        .iter()
        .map(|d| d.code)
        .collect::<Vec<_>>();
    assert_eq!(codes.first().copied(), Some("E0113"), "codes: {codes:?}");
}

#[test]
fn when_subtype_members_parse() {
    let declarations = expect_clean(
        "type mode_e: [IDLE, BUSY];\n\
         struct channel_s {\n    mode: mode_e;\n    fill: uint(bits: 8);\n    \
         when BUSY'kind channel_s {\n        payload: list of uint(bits: 8);\n        keep fill < 200;\n    };\n};\n",
    );
    assert_eq!(declarations.len(), 2);
    let Member::When(when) = &declarations[1].members[2] else {
        panic!("expected a when member");
    };
    assert_eq!(when.modifiers.len(), 2);
    assert_eq!(when.modifiers[0].value, "BUSY");
    assert_eq!(
        when.modifiers[0].id.as_ref().map(|(id, _)| id.as_str()),
        Some("kind")
    );
    // The base struct name is the optional trailing bare-id modifier.
    assert_eq!(when.modifiers[1].value, "channel_s");
    assert!(when.modifiers[1].id.is_none());
    assert_eq!(when.members.len(), 2);
}

#[test]
fn when_bare_and_chained_modifiers_parse() {
    let declarations = expect_clean(
        "struct s {\n    when fast'kind s'kind quick {\n        x: uint;\n    };\n};\n",
    );
    let Member::When(when) = &declarations[0].members[0] else {
        panic!("expected a when member");
    };
    assert_eq!(when.modifiers.len(), 3);
    assert_eq!(when.modifiers[0].value, "fast");
    assert_eq!(
        when.modifiers[0].id.as_ref().map(|(id, _)| id.as_str()),
        Some("kind")
    );
    assert!(when.modifiers[2].id.is_none());
    assert_eq!(when.modifiers[2].value, "quick");
}

#[test]
fn when_blocks_are_searched_for_fields_and_constraints() {
    // A `when` branch may shadow a base field (each subtype variant
    // carries its own field set), so only a duplicate in the SAME scope
    // is diagnosed.
    let shadowing =
        parse("struct s {\n    a: uint;\n    when fast'kind s {\n        a: uint;\n    };\n};\n");
    assert!(
        shadowing.diagnostics.is_empty(),
        "when-shadowing must be accepted: {:?}",
        shadowing
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
    let duplicate = parse("struct s {\n    a: uint;\n    a: uint;\n};\n");
    let rendered: Vec<String> = duplicate.diagnostics.iter().map(|d| d.render()).collect();
    assert_eq!(rendered, vec!["3:5: error[E0202]: duplicate field 'a'"]);
}

#[test]
fn while_break_and_continue_parse() {
    let declarations = expect_clean(
        "struct s {\n    i: uint;\n    run() is {\n        while i < 4 {\n            i = i + 1;\n            if i == 2 {\n                continue;\n            };\n            if i == 3 {\n                break;\n            };\n        };\n    };\n};\n",
    );
    let Member::Method(run) = &declarations[0].members[1] else {
        panic!("expected a method");
    };
    let Some(Stmt::While { cond, body, .. }) = run.body.first() else {
        panic!("expected a while statement");
    };
    let Expr::Binary {
        op: BinaryOp::Lt, ..
    } = cond
    else {
        panic!("expected a '<' condition");
    };
    assert_eq!(body.len(), 3);
    let Stmt::If { then_body, .. } = &body[1] else {
        panic!("expected an if statement");
    };
    assert!(matches!(then_body[0], Stmt::Continue { .. }));
    let Stmt::If { then_body, .. } = &body[2] else {
        panic!("expected an if statement");
    };
    assert!(matches!(then_body[0], Stmt::Break { .. }));
}

#[test]
fn case_statements_parse() {
    let declarations = expect_clean(
        "struct s {\n    opcode: uint;\n    run() is {\n        case opcode {\n            3: { out(3); };\n            [1, 2]: { out(12); };\n            default: { out(0); };\n        };\n    };\n};\n",
    );
    let Member::Method(run) = &declarations[0].members[1] else {
        panic!("expected a method");
    };
    let Some(Stmt::Case {
        scrutinee,
        arms,
        default,
        ..
    }) = run.body.first()
    else {
        panic!("expected a case statement");
    };
    let Expr::Ident { name, .. } = scrutinee else {
        panic!("expected an identifier scrutinee");
    };
    assert_eq!(name, "opcode");
    assert_eq!(arms.len(), 2);
    assert_eq!(arms[0].0.len(), 1);
    assert_eq!(arms[1].0.len(), 2, "the grouped alternative");
    assert!(default.is_some());
}

#[test]
fn ternary_expressions_parse() {
    let declarations = expect_clean(
        "struct s {\n    run() is {\n        var v: uint = a > 3 ? 10 : 20;\n    };\n};\n",
    );
    let Member::Method(run) = &declarations[0].members[0] else {
        panic!("expected a method");
    };
    let Some(Stmt::VarDecl {
        init: Some(init), ..
    }) = run.body.first()
    else {
        panic!("expected a var with initializer");
    };
    let Expr::Ternary {
        cond,
        then_expr,
        else_expr,
        ..
    } = init
    else {
        panic!("expected a ternary");
    };
    assert!(matches!(cond.as_ref(), Expr::Binary { .. }));
    assert!(matches!(then_expr.as_ref(), Expr::IntLiteral { .. }));
    assert!(matches!(else_expr.as_ref(), Expr::IntLiteral { .. }));
}

#[test]
fn if_then_variant_parses() {
    let declarations = expect_clean(
        "struct s {\n    run() is {\n        if 1 < 2 then {\n            out(1);\n        };\n    };\n};\n",
    );
    let Member::Method(run) = &declarations[0].members[0] else {
        panic!("expected a method");
    };
    assert!(matches!(&run.body[0], Stmt::If { .. }));
}

#[test]
fn method_keyword_spelling_parses() {
    let declarations = expect_clean(
        "struct s {\n    method run() is {\n        out(1);\n    };\n    method calc(x: uint): uint is empty;\n};\n",
    );
    let Member::Method(run) = &declarations[0].members[0] else {
        panic!("expected a method member");
    };
    assert_eq!(run.name, "run");
    let Member::Method(calc) = &declarations[0].members[1] else {
        panic!("expected a method member");
    };
    assert_eq!(calc.name, "calc");
    assert!(calc.body.is_empty(), "is empty must give an empty body");
}

#[test]
fn event_trigger_forms_parse() {
    let declarations = expect_clean(
        "unit u {\n    event clk is @sys.any;\n    event sampled is only cycle @p_sync.clk_e;\n    \
         event edge is change(dut.sig$) @sim;\n    event both is fall(clk$ == 1) @sim or rise(clk$) @sim;\n    \
         event port_edge is @off_p$;\n};\n",
    );
    let unit = &declarations[0];
    let Member::Event(event) = &unit.members[1] else {
        panic!("expected an event member");
    };
    let Some(trigger) = &event.trigger else {
        panic!("expected a trigger");
    };
    assert_eq!(trigger.modifier.as_deref(), Some("only"));
    assert!(trigger.cycle);
    assert_eq!(trigger.items[0].path, vec!["p_sync", "clk_e"]);

    let Member::Event(edge) = &unit.members[2] else {
        panic!("expected an event member");
    };
    let edge_trigger = edge.trigger.as_ref().unwrap();
    assert_eq!(edge_trigger.items[0].path, vec!["sim"]);
    assert!(edge_trigger.items[0].condition.is_some());

    let Member::Event(both) = &unit.members[3] else {
        panic!("expected an event member");
    };
    let both_trigger = both.trigger.as_ref().unwrap();
    assert_eq!(both_trigger.items.len(), 2, "or-composed items");
}

#[test]
fn keep_bind_constraints_parse() {
    let declarations = expect_clean(
        "unit u {\n    keep bind (sig_clk, external);\n    keep bind bfm0 to ~/dut/u0;\n};\n",
    );
    let Member::Constraint(first) = &declarations[0].members[0] else {
        panic!("expected a keep constraint");
    };
    assert!(first.is_bind);
    let Expr::Call { args, .. } = &first.expr else {
        panic!("expected a bind call");
    };
    assert_eq!(args.len(), 2);
    let Member::Constraint(second) = &declarations[0].members[1] else {
        panic!("expected a keep constraint");
    };
    assert!(second.is_bind);
}

#[test]
fn implication_not_and_soft_rhs_parse() {
    let declarations = expect_clean(
        "struct s {\n    a: bool;\n    b: bool;\n    keep a == TRUE => soft b == TRUE;\n    \
         keep not a and not b;\n};\n",
    );
    let Member::Constraint(first) = &declarations[0].members[2] else {
        panic!("expected a keep constraint");
    };
    let Expr::Binary {
        op: BinaryOp::Imply,
        ..
    } = first.expr
    else {
        panic!("expected an implication");
    };
    let Member::Constraint(second) = &declarations[0].members[3] else {
        panic!("expected a keep constraint");
    };
    assert!(matches!(
        second.expr,
        Expr::Binary {
            op: BinaryOp::LogicalAnd,
            ..
        }
    ));
}

#[test]
fn prefixed_field_declarations_parse() {
    let declarations =
        expect_clean("struct s {\n    !do_not_gen: bool;\n    %packed: uint(bits: 8);\n};\n");
    let Member::Field(no_gen) = &declarations[0].members[0] else {
        panic!("expected a field");
    };
    assert!(no_gen.no_gen && !no_gen.physical);
    let Member::Field(physical) = &declarations[0].members[1] else {
        panic!("expected a field");
    };
    assert!(physical.physical && !physical.no_gen);
}

#[test]
fn clause_4_5_places_string_match_between_identity_and_in() {
    let declarations = expect_clean(
        "struct s {\n    a: string;\n    b: string;\n    c: string;\n    \
         keep a ~ b;\n    keep a !~ b;\n    keep a === b ~ c;\n    \
         keep a == b ~ c;\n    keep a ~ b == c;\n};\n",
    );
    let keep = |index: usize| match &declarations[0].members[3 + index] {
        Member::Constraint(constraint) => &constraint.expr,
        _ => panic!("expected a keep constraint"),
    };
    let Expr::Binary {
        op: BinaryOp::Match,
        ..
    } = keep(0)
    else {
        panic!("expected a ~ b to parse as a match");
    };
    let Expr::Binary {
        op: BinaryOp::NotMatch,
        ..
    } = keep(1)
    else {
        panic!("expected a !~ b to parse as a negative match");
    };
    // `===` is tighter than `~` (clause 4.5): (a === b) ~ c
    let Expr::Binary {
        op: BinaryOp::Match,
        lhs,
        ..
    } = keep(2)
    else {
        panic!("expected the match on top of a === b ~ c");
    };
    assert!(matches!(
        lhs.as_ref(),
        Expr::Binary {
            op: BinaryOp::IdentityEq,
            ..
        }
    ));
    // `==` is tighter than `~`: (a == b) ~ c
    let Expr::Binary {
        op: BinaryOp::Match,
        lhs,
        ..
    } = keep(3)
    else {
        panic!("expected the match on top of a == b ~ c");
    };
    assert!(matches!(
        lhs.as_ref(),
        Expr::Binary {
            op: BinaryOp::Eq,
            ..
        }
    ));
    // ...and on the right: a ~ (b == c)
    let Expr::Binary {
        op: BinaryOp::Match,
        rhs,
        ..
    } = keep(4)
    else {
        panic!("expected the match on top of a ~ b == c");
    };
    assert!(matches!(
        rhs.as_ref(),
        Expr::Binary {
            op: BinaryOp::Eq,
            ..
        }
    ));
}

#[test]
fn clause_4_5_places_is_a_between_equality_and_comparisons() {
    let declarations = expect_clean(
        "struct s {\n    x: uint;\n    y: uint;\n    flag: bool;\n    \
         keep x is a uint;\n    keep x is not a uint;\n    keep x is a list of uint;\n    \
         keep flag == x is a uint;\n    keep x < y is a bool;\n};\n",
    );
    let keep = |index: usize| match &declarations[0].members[3 + index] {
        Member::Constraint(constraint) => &constraint.expr,
        _ => panic!("expected a keep constraint"),
    };
    let Expr::IsA {
        expr: _,
        ty,
        negated: false,
        ..
    } = keep(0)
    else {
        panic!("expected an is-a expression");
    };
    assert!(matches!(
        ty,
        TypeExpr::Scalar {
            base: ScalarBase::Uint,
            ..
        }
    ));
    let Expr::IsA {
        expr,
        negated: true,
        ..
    } = keep(1)
    else {
        panic!("expected a negated is-a expression");
    };
    assert!(matches!(expr.as_ref(), Expr::Ident { name, .. } if name == "x"));
    let Expr::IsA { ty, .. } = keep(2) else {
        panic!("expected an is-a expression");
    };
    assert!(matches!(ty, TypeExpr::List { .. }));
    // `is a` is tighter than `==` (clause 4.5): flag == (x is a uint)
    let Expr::Binary {
        op: BinaryOp::Eq,
        rhs,
        ..
    } = keep(3)
    else {
        panic!("expected == on top of flag == x is a uint");
    };
    assert!(matches!(rhs.as_ref(), Expr::IsA { .. }));
    // ...and looser than the comparisons: (x < y) is a bool
    let Expr::IsA { expr, .. } = keep(4) else {
        panic!("expected an is-a expression on top of x < y is a bool");
    };
    assert!(matches!(
        expr.as_ref(),
        Expr::Binary {
            op: BinaryOp::Lt,
            ..
        }
    ));
}

#[test]
fn bare_is_is_not_consumed_as_a_type_check() {
    // `is` followed by something other than `a`/`not a` is left alone;
    // the keep constraint then fails on its missing `;`, not on a
    // consumed `is`.
    let result = parse("struct s {\n    a: uint;\n    keep a is;\n};\n");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.render().contains("E0105")),
        "expected the keep terminator diagnostic: {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
}

#[test]
fn extend_global_is_rejected() {
    // Clause 4.3.1.1: extensions of `global` are reserved for the runtime.
    let result = parse("extend global {\n    helper: uint;\n};\n");
    assert!(result.declarations.is_empty());
    let rendered: Vec<String> = result.diagnostics.iter().map(|d| d.render()).collect();
    assert_eq!(
        rendered,
        vec![
            "1:8: error[E0116]: cannot extend 'global': extensions of the root type are reserved for the runtime"
                .to_owned()
        ]
    );
}

#[test]
fn file_level_define_and_import_still_form_the_global_pseudo_declaration() {
    // The parser's own `extend global` pseudo-declaration (the holder of
    // file-level `define`/`import` members) is not user syntax and must
    // keep parsing.
    let declarations =
        expect_clean("define WIDTH 8;\nimport pkg;\nstruct s {\n    w: uint(bits: WIDTH);\n};\n");
    let pseudo = declarations
        .iter()
        .find(|d| d.kind == DeclarationKind::Extend && d.name == "global")
        .expect("expected the global pseudo-declaration");
    assert_eq!(pseudo.members.len(), 2);
}

#[test]
fn on_member_parses_with_path_and_body() {
    let declarations = expect_clean(
        "unit env {\n    event reset_done;\n    on reset_done {\n        out(\"fired\");\n    };\n\
         on driver.clock {\n        out(\"tick\");\n    };\n};\n",
    );
    let Member::On(first) = &declarations[0].members[1] else {
        panic!("expected an on member");
    };
    assert_eq!(first.path, vec!["reset_done"]);
    assert_eq!(first.body.len(), 1);
    let Member::On(second) = &declarations[0].members[2] else {
        panic!("expected an on member");
    };
    assert_eq!(second.path, vec!["driver", "clock"]);
    assert_eq!(second.body.len(), 1);
}

#[test]
fn hash_directives_include_and_exclude_at_member_level() {
    // `#ifdef` of an undefined name is excluded ("ignore it", clause
    // 21.2): garbage inside the braces must not produce diagnostics.
    // `#define` registers the flag; the `#ifndef` then excludes and the
    // following `#ifdef` includes a real member. `#undef` clears it.
    let declarations = expect_clean(
        "struct s {\n    \
         #ifdef SAVE_THE_RUN {\n        this is not e at all !!!\n    };\n    \
         #define SAVE_THE_RUN\n    \
         #ifdef SAVE_THE_RUN {\n        saved: bool;\n    };\n    \
         #ifndef SAVE_THE_RUN {\n        not_saved: bool;\n    };\n    \
         #undef SAVE_THE_RUN;\n    \
         x: uint;\n};\n",
    );
    // `saved` (included after the define) and `x`; the excluded branches
    // contribute nothing.
    assert_eq!(declarations.len(), 1);
    assert_eq!(declarations[0].name, "s");
    assert_eq!(declarations[0].members.len(), 2);
}

#[test]
fn hash_directives_include_and_exclude_at_action_level() {
    let declarations = expect_clean(
        "extend sys {\n    run() is also {\n        #ifdef MISSING {\n            \
         out(\"no\");\n        };\n        #ifndef MISSING {\n            out(\"yes\");\n        };\n\
         #define MISSING\n        #ifdef MISSING {\n            out(\"flagged\");\n        };\n\
         #undef MISSING\n    };\n};\n",
    );
    let Member::Method(run) = &declarations[0].members[0] else {
        panic!("expected a method");
    };
    // Two branches include: `#ifndef MISSING` before the define and
    // `#ifdef MISSING` after it; the excluded branches contribute none.
    assert_eq!(run.body.len(), 2);
}

#[test]
fn conditional_directive_at_file_level_is_rejected() {
    // Clause 21.2 lists the conditional forms as statement, struct-member
    // and action positions; the module level takes `#define`/`#undef`.
    let result = parse("#ifdef FLAG {\nstruct s {\n    x: uint;\n};\n};\n");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.render().contains("E0117")),
        "expected E0117: {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
}

#[test]
fn macro_definitions_record_header_pattern_and_body() {
    // Clause 15.2/15.3 definitions are recorded uninterpreted: the
    // match-expression grammar and the expansion engine are later
    // milestones. Bodies are raw tokens — constructs the parser does
    // not know (the macro teaches it) must not produce diagnostics.
    let declarations = expect_clean(
        "define <e_util_min_value'exp> \"MIN_VALUE[ ]\\(<type>\\)\" as {\n    \
         set_of_values(<type>).uint_min().as_a(<type>);\n};\n\
         define <e_util_range'exp> \"RANGE[ ]\\(<type>\\)\" as computed {\n    \
         if the_type is a rf_numeric (rfn) {\n        return set.uint_size();\n    };\n};\n\
         define <plain_macro> \"NO_KIND[ ]\" as {\n    out(\"plain\");\n};\n",
    );
    let pseudo = declarations
        .iter()
        .find(|d| d.kind == DeclarationKind::Extend && d.name == "global")
        .expect("expected the global pseudo-declaration");
    assert_eq!(pseudo.members.len(), 3);
    let Member::Macro(first) = &pseudo.members[0] else {
        panic!("expected a macro definition");
    };
    assert_eq!(first.name, "e_util_min_value");
    assert_eq!(first.category.as_deref(), Some("exp"));
    assert_eq!(first.pattern, r"MIN_VALUE[ ]\(<type>\)");
    assert!(!first.computed);
    // The body tokens exclude the enclosing braces: identifiers,
    // punctuation, `<`, `>` and the call.
    assert!(first.body.len() >= 10);
    let Member::Macro(second) = &pseudo.members[1] else {
        panic!("expected a macro definition");
    };
    assert!(second.computed);
    // The computed body nests braces; the capture must survive them.
    assert!(second.body.iter().any(|token| matches!(
        token.kind,
        e1647::lexer::TokenKind::Identifier(ref word) if word == "rf_numeric"
    )));
    let Member::Macro(plain) = &pseudo.members[2] else {
        panic!("expected a macro definition");
    };
    assert_eq!(plain.category, None);
}

#[test]
fn scalar_type_declaration_records_underlying() {
    // `type name: <type>;` (clause 5) — the representation is recorded;
    // unknown names in it are well-formedness errors like any type use.
    let declarations = expect_clean("type addr_t : int (bits : 8);\n");
    assert_eq!(declarations[0].kind, DeclarationKind::Type);
    let Some(underlying) = &declarations[0].underlying else {
        panic!("expected the underlying representation");
    };
    assert!(matches!(
        underlying,
        TypeExpr::Scalar {
            base: ScalarBase::Int,
            ..
        }
    ));
    let result = parse("type bad_t : no_such_type;\n");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.render().contains("E0203")),
        "expected the unknown underlying type to be rejected: {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
}

#[test]
fn template_declarations_record_params_and_parameterized_bases() {
    // Clause 8.1: `template struct name of <params> like base { .. }` —
    // parameters (with tag'category and defaults), `package` prefixes,
    // and parameterized bases are recorded. Instantiation is a later
    // milestone.
    let declarations = expect_clean(
        "struct base_struct {\n    id: int;\n};\n\
         template struct iterator of <type> like base_struct {\n    idx: int;\n};\n\
         package template struct map of (<key'type>, <value'type> = <key'type>) like base_struct {\n    \
         private m: list of <value'type>;\n    \
         put(k: <key'type>, v: <value'type>) is { };\n};\n\
         struct queue_s like iterator of int {\n    n: int;\n};\n",
    );
    assert_eq!(declarations.len(), 4);
    let map = &declarations[2];
    assert!(map.packaged, "package prefix recorded");
    assert_eq!(map.template_params.len(), 2);
    assert_eq!(map.template_params[0].name, "key'type");
    assert_eq!(map.template_params[1].name, "value'type");
    assert!(map.template_params[1].default.is_some());
    assert!(map.template_params[0].default.is_none());
    assert_eq!(map.like.as_deref(), Some("base_struct"));
    // The parameterized base of a plain struct is recorded with its
    // arguments.
    let queue = &declarations[3];
    let Some(TypeExpr::TemplateRef { name, args, .. }) = &queue.like_type else {
        panic!("expected a template-instance base");
    };
    assert_eq!(name, "iterator");
    assert_eq!(args.len(), 1);
    // The private member prefix is recorded on the field.
    let Member::Field(m) = &map.members[0] else {
        panic!("expected a field");
    };
    assert_eq!(m.visibility.as_deref(), Some("private"));
}

#[test]
fn template_body_types_record_formals_and_refs() {
    let declarations = expect_clean(
        "template struct container of <type> {\n    \
         items: list of <type>;\n    \
         keyed: list (key:it) of <type>;\n    \
         pick(i: uint): <type> is { return items[i]; };\n};\n",
    );
    let Member::Field(items) = &declarations[0].members[0] else {
        panic!("expected a field");
    };
    let TypeExpr::List { element, key, .. } = &items.ty else {
        panic!("expected a list type");
    };
    assert!(key.is_none());
    assert!(matches!(element.as_ref(), TypeExpr::Formal { name, .. } if name == "type"));
    let Member::Field(keyed) = &declarations[0].members[1] else {
        panic!("expected a field");
    };
    let TypeExpr::List { key, .. } = &keyed.ty else {
        panic!("expected a keyed list type");
    };
    assert!(key.is_some(), "the keyed-list key expression is recorded");
}

#[test]
fn is_a_binds_the_matched_value() {
    // `if p is a compound_packet (cp)` — the binding variable is part of
    // the type check expression (clause 5).
    let declarations = expect_clean(
        "struct packet_s {\n    x: int;\n};\n\
         extend sys {\n    run() is also {\n        var p: packet_s;\n        \
         if p is a packet_s (cp) {\n            out(cp.x);\n        };\n    };\n};\n",
    );
    let Member::Method(run) = &declarations[1].members[0] else {
        panic!("expected a method");
    };
    let Some(Stmt::If { cond, .. }) = run.body.iter().find(|s| matches!(s, Stmt::If { .. })) else {
        panic!("expected an if");
    };
    let Expr::IsA { binding, .. } = cond else {
        panic!("expected an is-a condition");
    };
    assert_eq!(binding.as_deref(), Some("cp"));
}

#[test]
fn expect_and_assume_members_record_te_and_else() {
    // Clause 11.4: the temporal expression is recorded as raw tokens —
    // the clause 12 grammar is a later milestone — and the `else`
    // failure body parses as ordinary actions, in both the one-statement
    // and the braced form.
    let declarations = expect_clean(
        "extend sys {\n    event enter_exec_st is @sys.any;\n    \
         expect @enter_exec_st => {@fetch1_assert} @sys.any else\n        \
         dut_error(\"PROTOCOL ERROR\");\n    \
         assume @enter_exec_st => {@fetch1_assert} @sys.any else {\n            \
         dut_error(\"ASSUMPTION\");\n        };\n};\n",
    );
    let Member::Expect(expect) = &declarations[0].members[1] else {
        panic!("expected an expect member");
    };
    assert_eq!(expect.kind, e1647::ast::ExpectKind::Expect);
    // `@enter_exec_st => {@fetch1_assert} @sys.any` structures as
    // Sampled(Yield(@enter_exec_st, Sequence{@fetch1_assert}), @sys.any)
    // (clause 12.2.13 yield, 12.2.8 sequence, 12.2.5 sampling).
    let shape = match &expect.te {
        // The sampling `@sys.any` attaches lexically to the yield's rhs;
        // the semantic propagation to both sides (12.1.4, 12.2.13) is
        // the temporal engine's sampled normal form, a later milestone.
        e1647::ast::TemporalExpr::Yield { lhs, rhs } => match (&**lhs, &**rhs) {
            (
                e1647::ast::TemporalExpr::EventRef { path },
                e1647::ast::TemporalExpr::Sampled { te, .. },
            ) => {
                path == &vec!["enter_exec_st".to_owned()]
                    && matches!(
                        &**te,
                        e1647::ast::TemporalExpr::Sequence { items } if items.len() == 1
                    )
            }
            _ => false,
        },
        _ => false,
    };
    assert!(shape, "te shape: {:?}", expect.te);
    assert_eq!(expect.else_body.len(), 1);
    let Member::Expect(assumption) = &declarations[0].members[2] else {
        panic!("expected an assume member");
    };
    assert_eq!(assumption.kind, e1647::ast::ExpectKind::Assume);
    assert_eq!(assumption.else_body.len(), 1);
}

#[test]
fn temporal_grammar_covers_the_clause_12_operators() {
    // The clause 12 operators the grammar records: sequences, sampling,
    // yields, repeats, propositions, edge samples, and/or/not/fail/
    // eventually.
    let declarations = expect_clean(
        "extend sys {\n    ev: event;\n    clk: event;\n    \
         expect {[2..4]*@ev; @clk} @clk else dut_error(\"SEQ\");\n    \
         expect ~[1..3]*true(sig == 1) @clk;\n    \
         expect (change(sig) or rise(sig)) and not @ev;\n    \
         expect fail{@ev; @ev} and eventually @clk;\n    \
         expect [2]*cycle @clk;\n};\n",
    );
    let extend = &declarations[0];
    let Member::Expect(first_match_seq) = &extend.members[2] else {
        panic!("expected the sequence expect");
    };
    let seq_shape = match &first_match_seq.te {
        e1647::ast::TemporalExpr::Sampled { te, .. } => match &**te {
            e1647::ast::TemporalExpr::Sequence { items } => {
                items.len() == 2
                    && matches!(
                        &items[0],
                        e1647::ast::TemporalExpr::Repeat {
                            kind: e1647::ast::RepeatKind::FirstMatch,
                            ..
                        }
                    )
            }
            _ => false,
        },
        _ => false,
    };
    assert!(seq_shape, "te: {:?}", first_match_seq.te);
    let Member::Expect(true_match) = &extend.members[3] else {
        panic!("expected the repeat expect");
    };
    let repeat_shape = match &true_match.te {
        e1647::ast::TemporalExpr::Sampled { te, .. } => match &**te {
            e1647::ast::TemporalExpr::Repeat { kind, .. } => {
                *kind == e1647::ast::RepeatKind::TrueMatch
            }
            _ => false,
        },
        _ => false,
    };
    assert!(repeat_shape);
    let Member::Expect(or_edge) = &extend.members[4] else {
        panic!("expected the edge-sample expect");
    };
    let edge_shape = match &or_edge.te {
        e1647::ast::TemporalExpr::And(lhs, rhs) => {
            // The parenthesized or-group arrives as a Group.
            let lhs_ok = match &**lhs {
                e1647::ast::TemporalExpr::Group(inner) => match &**inner {
                    e1647::ast::TemporalExpr::Or(a, b) => {
                        matches!(
                            &**a,
                            e1647::ast::TemporalExpr::EdgeSample {
                                routine: e1647::ast::EdgeRoutine::Change,
                                ..
                            }
                        ) && matches!(
                            &**b,
                            e1647::ast::TemporalExpr::EdgeSample {
                                routine: e1647::ast::EdgeRoutine::Rise,
                                ..
                            }
                        )
                    }
                    _ => false,
                },
                _ => false,
            };
            lhs_ok && matches!(&**rhs, e1647::ast::TemporalExpr::Not(..))
        }
        _ => false,
    };
    assert!(edge_shape);
    let Member::Expect(fail_eventually) = &extend.members[5] else {
        panic!("expected the fail/eventually expect");
    };
    let fail_shape = match &fail_eventually.te {
        e1647::ast::TemporalExpr::And(lhs, rhs) => {
            matches!(&**lhs, e1647::ast::TemporalExpr::Fail(..))
                && matches!(&**rhs, e1647::ast::TemporalExpr::Eventually(..))
        }
        _ => false,
    };
    assert!(fail_shape);
    let Member::Expect(cycle_repeat) = &extend.members[6] else {
        panic!("expected the cycle repeat expect");
    };
    let cycle_shape = match &cycle_repeat.te {
        e1647::ast::TemporalExpr::Sampled { te, .. } => match &**te {
            // `[2]*cycle` is the fixed repeat (12.2.9's syntax example).
            e1647::ast::TemporalExpr::Repeat {
                kind, te: inner, ..
            } => {
                *kind == e1647::ast::RepeatKind::Fixed
                    && matches!(&**inner, e1647::ast::TemporalExpr::Cycle)
            }
            _ => false,
        },
        _ => false,
    };
    assert!(cycle_shape);
}

#[test]
fn expect_without_else_and_missing_te_report() {
    // The `else` clause is optional; an empty temporal expression is
    // rejected with its own code.
    let declarations = expect_clean("extend sys {\n    ev: event;\n    expect @ev;\n};\n");
    let Member::Expect(expect) = &declarations[0].members[1] else {
        panic!("expected an expect member");
    };
    assert!(expect.else_body.is_empty());
    assert!(matches!(
        expect.te,
        e1647::ast::TemporalExpr::EventRef { .. }
    ));
    let result = parse("extend sys {\n    expect;\n};\n");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.render().contains("E0120")),
        "expected E0120 for the missing temporal expression: {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
}

#[test]
fn select_distributions_parse_weights_and_verbatim_choices() {
    // Clause 10: `select { <weight> : <choice>; .. }` — weights parse,
    // choices are recorded verbatim (paren-aware so `normal(a, b)`
    // keeps its arguments), and the whole constraint is a no-op for the
    // rejection sampler until the distribution arc.
    let declarations = expect_clean(
        "type addr_t : uint (bits : 16);\n\
         struct pkt {\n    address : addr_t;\n};\n\
         extend pkt {\n    keep soft address == select {\n        50 : min;\n        \
         25 : max;\n        25 : others;\n    };\n    \
         keep soft address == select {\n        100 : normal(MAX_UINT/2, MAX_UINT/10);\n    };\n    \
         keep soft address == select {\n        50 : normal(2, 10);\n        50 : edges;\n    };\n};\n",
    );
    let extend = &declarations[2];
    let Member::Constraint(first) = &extend.members[0] else {
        panic!("expected a keep constraint");
    };
    let Expr::Binary {
        op: BinaryOp::Eq,
        rhs,
        ..
    } = &first.expr
    else {
        panic!("expected the equality against the select");
    };
    let Expr::Select { arms, .. } = rhs.as_ref() else {
        panic!("expected a select on the right side");
    };
    assert_eq!(arms.len(), 3);
    let weights: Vec<&str> = arms
        .iter()
        .map(|arm| match &arm.weight {
            Expr::IntLiteral { text, .. } => text.as_str(),
            _ => "?",
        })
        .collect();
    assert_eq!(weights, vec!["50", "25", "25"]);
    let choice_texts: Vec<Vec<String>> = arms
        .iter()
        .map(|arm| {
            arm.choice
                .iter()
                .map(|token| match &token.kind {
                    e1647::lexer::TokenKind::Identifier(word) => word.clone(),
                    e1647::lexer::TokenKind::Punctuation(p) => p.clone(),
                    e1647::lexer::TokenKind::Keyword(word) => word.clone(),
                    _ => "?".to_owned(),
                })
                .collect()
        })
        .collect();
    assert_eq!(choice_texts[0], vec!["min"]);
    assert_eq!(choice_texts[1], vec!["max"]);
    assert_eq!(choice_texts[2], vec!["others"]);
    let Member::Constraint(second) = &extend.members[1] else {
        panic!("expected a keep constraint");
    };
    let Expr::Binary { rhs, .. } = &second.expr else {
        panic!("expected an equality");
    };
    let Expr::Select { arms, .. } = rhs.as_ref() else {
        panic!("expected a select");
    };
    // `normal(MAX_UINT/2, MAX_UINT/10)` keeps its commas and slashes.
    let normal_texts: Vec<String> = arms[0]
        .choice
        .iter()
        .map(|token| match &token.kind {
            e1647::lexer::TokenKind::Identifier(word) => word.clone(),
            e1647::lexer::TokenKind::Number(text) => text.clone(),
            e1647::lexer::TokenKind::Punctuation(p) => p.clone(),
            _ => "?".to_owned(),
        })
        .collect();
    assert_eq!(
        normal_texts,
        vec![
            "normal", "(", "MAX_UINT", "/", "2", ",", "MAX_UINT", "/", "10", ")"
        ]
    );
}

#[test]
fn wait_statements_split_numeric_from_temporal_forms() {
    // 13.1.3: `cycle`, `[n]` and `[n]*cycle` stay the numeric wait;
    // event atoms, `until` + TE, repeats over events, sequences and
    // proposition waits are temporal.
    let declarations = expect_clean(
        "struct s {\n    ev: event;\n    clk: event;\n    x: uint;\n    run() is {\n        \
         wait cycle;\n        wait [3];\n        wait [3]*cycle;\n        wait until x > 3;\n        \
         wait @ev;\n        wait until @ev;\n        wait [2]*@ev;\n        \
         wait {@ev; @clk} @clk;\n        wait true(x == 3) @clk;\n    };\n};\n",
    );
    let Member::Method(run) = &declarations[0].members[3] else {
        panic!("expected the run method");
    };
    assert_eq!(run.body.len(), 9);
    // The numeric forms keep their cycle-count normalization.
    for stmt in run.body.iter().take(4) {
        assert!(
            matches!(stmt, Stmt::Wait { .. }),
            "expected a numeric wait: {stmt:?}"
        );
    }
    // `wait @ev` and `wait until @ev` are the same event-atom TE.
    for stmt in run.body.iter().skip(4).take(2) {
        let Stmt::WaitTemporal { te, .. } = stmt else {
            panic!("expected a temporal wait: {stmt:?}");
        };
        assert!(
            matches!(te, e1647::ast::TemporalExpr::EventRef { .. }),
            "expected an event atom: {te:?}"
        );
    }
    // `wait [2]*@ev` is the fixed repeat (12.2.9).
    let Stmt::WaitTemporal { te, .. } = &run.body[6] else {
        panic!("expected the repeat wait");
    };
    let e1647::ast::TemporalExpr::Repeat {
        kind: e1647::ast::RepeatKind::Fixed,
        ..
    } = te
    else {
        panic!("expected a fixed repeat: {te:?}");
    };
    // A sampled sequence and a proposition keep their sampling.
    for stmt in run.body.iter().skip(7) {
        let Stmt::WaitTemporal { te, .. } = stmt else {
            panic!("expected a temporal wait: {stmt:?}");
        };
        let e1647::ast::TemporalExpr::Sampled { te: inner, .. } = te else {
            panic!("expected the sampling suffix: {te:?}");
        };
        assert!(
            matches!(
                &**inner,
                e1647::ast::TemporalExpr::Sequence { .. } | e1647::ast::TemporalExpr::True { .. }
            ),
            "expected a sequence or proposition: {inner:?}"
        );
    }
}

#[test]
fn sync_statements_parse_bare_and_temporal_forms() {
    // 13.1.2: `sync;` / `sync cycle;` are the bare synchronization
    // action (no TE); `sync <te>;` carries a temporal expression.
    let declarations = expect_clean(
        "struct s {\n    ev: event;\n    clk: event;\n    run() is {\n        \
         sync;\n        sync cycle;\n        sync @ev;\n        sync {@ev; @clk} @clk;\n    };\n};\n",
    );
    let Member::Method(run) = &declarations[0].members[2] else {
        panic!("expected the run method");
    };
    assert_eq!(run.body.len(), 4);
    for stmt in run.body.iter().take(2) {
        let Stmt::Sync { te, .. } = stmt else {
            panic!("expected a bare sync: {stmt:?}");
        };
        assert!(te.is_none(), "the bare sync carries no TE: {stmt:?}");
    }
    for stmt in run.body.iter().skip(2) {
        let Stmt::Sync { te, .. } = stmt else {
            panic!("expected a temporal sync: {stmt:?}");
        };
        let Some(te) = te else {
            panic!("expected a TE: {stmt:?}");
        };
        assert!(
            matches!(
                te,
                e1647::ast::TemporalExpr::EventRef { .. }
                    | e1647::ast::TemporalExpr::Sampled { .. }
            ),
            "expected an event atom or sampled sequence: {te:?}"
        );
    }
}
