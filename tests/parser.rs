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
    let result = parse("struct s {\n    b: uint;\n    cover my_cover is { item a; };\n};\n");
    assert_eq!(
        result
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec!["E0113", "E0101"]
    );
    // The parser recovers; the well-formed field survives in the partial
    // AST and the diagnostics are authoritative.
    assert_eq!(result.declarations[0].members.len(), 1);
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
