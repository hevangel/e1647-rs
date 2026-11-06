use e1647::ast::DeclarationKind;
use e1647::elab::{Elaborated, ElaboratedType, elaborate};
use e1647::parser::parse;

fn elaborated(source: &str) -> Elaborated {
    let parsed = parse(source);
    assert!(
        parsed.diagnostics.is_empty(),
        "parse diagnostics: {:?}",
        parsed
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
    let result = elaborate(&parsed.declarations);
    assert!(
        result.diagnostics.is_empty(),
        "elaboration diagnostics: {:?}",
        result
            .diagnostics
            .iter()
            .map(|d| d.render())
            .collect::<Vec<_>>()
    );
    result
}

fn field_names(elaborated_type: &ElaboratedType) -> Vec<&str> {
    elaborated_type
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .collect()
}

fn find<'a>(result: &'a Elaborated, name: &str) -> &'a ElaboratedType {
    result
        .types
        .iter()
        .find(|ty| ty.name == name)
        .unwrap_or_else(|| panic!("type '{name}' not elaborated"))
}

#[test]
fn like_chain_composes_base_members_first() {
    let result = elaborated(
        "struct packet { addr: uint; };\n\
         struct frame like packet { crc: uint; };\n\
         struct ack_frame like frame { tag: uint; };\n",
    );
    let ack = find(&result, "ack_frame");
    assert_eq!(field_names(ack), vec!["addr", "crc", "tag"]);
    assert_eq!(ack.base.as_deref(), Some("frame"));
    let frame = find(&result, "frame");
    assert_eq!(field_names(frame), vec!["addr", "crc"]);
}

#[test]
fn extends_append_in_source_order_after_like_members() {
    let result = elaborated(
        "struct packet { addr: uint; };\n\
         extend packet { keep addr < 10; };\n\
         struct frame like packet { crc: uint; };\n\
         extend frame { len: uint; };\n",
    );
    let packet = find(&result, "packet");
    assert_eq!(field_names(packet), vec!["addr"]);
    assert_eq!(packet.constraints.len(), 1);
    let frame = find(&result, "frame");
    assert_eq!(field_names(frame), vec!["addr", "crc", "len"]);
    // The like chain also carries packet's composed constraints.
    assert_eq!(frame.constraints.len(), 1);
}

#[test]
fn extends_before_their_target_compose_the_same() {
    // e is order-independent: the extend may precede its target.
    let forward = elaborated("extend packet { extra: uint; };\nstruct packet { addr: uint; };\n");
    let backward = elaborated("struct packet { addr: uint; };\nextend packet { extra: uint; };\n");
    assert_eq!(field_names(find(&forward, "packet")), vec!["addr", "extra"]);
    assert_eq!(
        field_names(find(&backward, "packet")),
        field_names(find(&forward, "packet"))
    );
}

#[test]
fn when_members_are_carried_ancestor_first() {
    let result = elaborated(
        "type mode_e: [IDLE, BUSY];\n\
         struct base_s { mode: mode_e; when BUSY'kind base_s { fill: uint; } };\n\
         struct derived like base_s { tag: uint; };\n",
    );
    let base = find(&result, "base_s");
    assert_eq!(base.whens.len(), 1);
    assert_eq!(base.whens[0].modifiers[0].value, "BUSY");
    let derived = find(&result, "derived");
    assert_eq!(derived.whens.len(), 1, "the base's when is inherited");
}

#[test]
fn like_cycle_is_reported_once_per_type() {
    let parsed = parse("struct a like b { x: uint; };\nstruct b like a { y: uint; };\n");
    assert!(parsed.diagnostics.is_empty());
    let result = elaborate(&parsed.declarations);
    let codes: Vec<&str> = result.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec!["E0301", "E0301"]);
}

#[test]
fn duplicate_field_across_extend_blocks_is_reported_once() {
    let parsed = parse("struct s { a: uint; };\nextend s { b: uint; };\nextend s { b: uint; };\n");
    assert!(parsed.diagnostics.is_empty());
    let result = elaborate(&parsed.declarations);
    let rendered: Vec<String> = result.diagnostics.iter().map(|d| d.render()).collect();
    assert_eq!(
        rendered,
        vec!["3:12: error[E0302]: duplicate field 'b' in elaborated type 's'"]
    );
}

#[test]
fn duplicate_field_across_like_chain_is_reported() {
    let parsed = parse("struct base_s { x: uint; };\nstruct derived like base_s { x: uint; };\n");
    assert!(parsed.diagnostics.is_empty());
    let result = elaborate(&parsed.declarations);
    let rendered: Vec<String> = result.diagnostics.iter().map(|d| d.render()).collect();
    assert_eq!(
        rendered,
        vec!["2:30: error[E0302]: duplicate field 'x' in elaborated type 'derived'"]
    );
}

#[test]
fn method_redefinition_requires_modifier() {
    let parsed = parse("struct s { run() is { }; };\nextend s { run() is { }; };\n");
    assert!(parsed.diagnostics.is_empty());
    let result = elaborate(&parsed.declarations);
    let rendered: Vec<String> = result.diagnostics.iter().map(|d| d.render()).collect();
    assert_eq!(
        rendered,
        vec![
            "2:12: error[E0303]: method 'run' is redefined in elaborated type 's' without 'is also', 'is first' or 'is only'"
        ]
    );
    // `is also` is the legal extension form.
    let clean = elaborated("struct s { run() is { }; };\nextend s { run() is also { }; };\n");
    assert_eq!(find(&clean, "s").methods.len(), 2);
}

#[test]
fn type_declarations_pass_through_enumerals() {
    let result = elaborated("type color_e: [RED, GREEN = 2];\n");
    assert_eq!(result.types.len(), 1);
    assert_eq!(result.types[0].kind, DeclarationKind::Type);
    assert_eq!(result.types[0].enumerals.len(), 2);
    assert_eq!(result.types[0].enumerals[1].name, "GREEN");
    assert!(result.types[0].enumerals[1].value.is_some());
    assert!(result.types[0].fields.is_empty());
}

#[test]
fn builtin_sys_is_synthesized_from_its_extends() {
    let result = elaborated("extend sys {\n    started: bool;\n    run() is also { };\n};\n");
    let sys = find(&result, "sys");
    assert_eq!(sys.kind, DeclarationKind::Unit);
    assert_eq!(field_names(sys), vec!["started"]);
    assert_eq!(sys.methods.len(), 1);
    assert_eq!(sys.methods[0].name, "run");
}

#[test]
fn on_members_compose_through_extends_in_order() {
    // Repeating an `on` declaration extends it (clause 11.2, the
    // `is also` effect): the composed list keeps every block, base types
    // first, extends in declaration order.
    let result = elaborated(
        "unit env {\n    event reset_done;\n};\n\
         extend env {\n    on reset_done { out(\"one\"); };\n};\n\
         extend env {\n    on reset_done { out(\"two\"); };\n};\n",
    );
    let env = find(&result, "env");
    assert_eq!(env.ons.len(), 2);
    assert_eq!(env.ons[0].path, vec!["reset_done"]);
    assert_eq!(env.ons[1].path, vec!["reset_done"]);
    assert_eq!(env.ons[0].body.len(), 1);
    assert_eq!(env.ons[1].body.len(), 1);
}

#[test]
fn when_blocks_synthesize_subtype_views() {
    // 6.7: a single-modifier `when VALUE'id` block synthesizes a
    // subtype view named `VALUE base` — the base's composed members
    // with the when block's appended — marked with its discriminator.
    let result = elaborated(
        "type mode_t : [LO, HI];\n\
         struct pkt {\n    kind: mode_t;\n    v: uint (bits: 8);\n    \
         when LO kind is {\n            keep soft v == select { 100 : min; };\n        };\n    \
         when HI kind is {\n            keep soft v == select { 100 : max; };\n        };\n};\n",
    );
    let view = find(&result, "LO pkt");
    assert_eq!(view.when_of, vec![("kind".to_owned(), "LO".to_owned())]);
    assert_eq!(view.base, Some("pkt".to_owned()));
    // The view carries the base's fields plus the when constraint.
    assert_eq!(view.fields.len(), 2);
    assert_eq!(view.constraints.len(), 1);
    let hi = find(&result, "HI pkt");
    assert_eq!(hi.when_of, vec![("kind".to_owned(), "HI".to_owned())]);
    assert_eq!(hi.constraints.len(), 1);
    // The base itself stays unmarked and without the when constraints.
    let base = find(&result, "pkt");
    assert_eq!(base.when_of, Vec::<(String, String)>::new());
    assert_eq!(base.constraints.len(), 0);
}

#[test]
fn chained_modifiers_synthesize_a_combined_view() {
    // 6.7.1: subtypes can be combinations of fields — a when block
    // whose `value'id` modifiers each name their determinant field
    // synthesizes ONE conjunctive view named with all the values
    // space-joined in modifier order, carrying every (field, value)
    // pair.
    let result = elaborated(
        "type mode_t : [LO, HI];\n\
         type dir_t : [UP, DOWN];\n\
         struct pkt {\n    kind: mode_t;\n    sel: dir_t;\n    v: uint (bits: 8);\n    \
         when LO'kind HI'sel is {\n            keep soft v == select { 100 : min; };\n        };\n};\n",
    );
    let view = find(&result, "LO HI pkt");
    assert_eq!(
        view.when_of,
        vec![
            ("kind".to_owned(), "LO".to_owned()),
            ("sel".to_owned(), "HI".to_owned())
        ]
    );
    assert_eq!(view.base, Some("pkt".to_owned()));
    assert_eq!(view.fields.len(), 3);
    assert_eq!(view.constraints.len(), 1);
    // The base itself stays unmarked.
    assert_eq!(find(&result, "pkt").when_of, Vec::<(String, String)>::new());
}

#[test]
fn nested_when_blocks_compose_two_level_views() {
    // A `when` member inside a when block applies ON TOP of the
    // enclosing view: the outer view is synthesized first, then the
    // inner when's modifiers run against the synthesized view's fields,
    // producing a further view named with the combined values and
    // carrying BOTH levels' members. The combined view's base chain
    // walks the outer view to the base type.
    let result = elaborated(
        "type mode_t : [LO, HI];\n\
         type dir_t : [UP, DOWN];\n\
         struct pkt {\n    kind: mode_t;\n    dir: dir_t;\n    v: uint (bits: 8);\n    w: uint (bits: 8);\n    \
         when LO kind is {\n            keep soft v == select { 100 : min; };\n        \
         when HI dir is {\n                keep soft w == select { 100 : max; };\n            };\n        };\n};\n",
    );
    // The outer view exists on its own.
    let outer = find(&result, "LO pkt");
    assert_eq!(outer.when_of, vec![("kind".to_owned(), "LO".to_owned())]);
    assert_eq!(outer.constraints.len(), 1);
    // The combined view carries both levels' pairs and constraints.
    let combined = find(&result, "LO HI pkt");
    assert_eq!(
        combined.when_of,
        vec![
            ("kind".to_owned(), "LO".to_owned()),
            ("dir".to_owned(), "HI".to_owned())
        ]
    );
    assert_eq!(combined.base, Some("LO pkt".to_owned()));
    assert_eq!(combined.constraints.len(), 2);
    assert_eq!(combined.fields.len(), 4);
}

#[test]
fn template_instances_monomorphize() {
    // 8.3: a template instance creates a new struct by substituting the
    // actuals for the formals — in field types, method signatures and
    // the parameterized base; two instances with the same actuals are
    // the same instance.
    let result = elaborated(
        "struct base_struct {\n    id: int;\n};\n\
         template struct iterator of <type> like base_struct {\n    idx: int;\n};\n\
         template struct container of <type> like base_struct {\n    \
         get_iterator(): iterator of <type> is undefined;\n    \
         add(item: <type>) is undefined;\n};\n\
         extend sys {\n    c: container of (int);\n};\n",
    );
    // The used instance materialized, with its parameterized base.
    let view = find(&result, "container of (int)");
    assert_eq!(view.base, Some("base_struct".to_owned()));
    // The method signature's formal substituted to the actual.
    let add = view
        .methods
        .iter()
        .find(|method| method.name == "add")
        .expect("the add method");
    assert_eq!(
        add.params[0].ty.as_ref().map(|ty| ty.render()),
        Some("int".to_owned())
    );
    // The substituted `iterator of <type>` reference materialized its
    // own instance (the worklist feeds itself); like composition gives
    // it the base's `id` plus its own `idx`.
    let iterator = find(&result, "iterator of (int)");
    assert_eq!(iterator.base, Some("base_struct".to_owned()));
    assert_eq!(iterator.fields.len(), 2);
    // Same actuals, same instance: exactly one view exists.
    assert_eq!(
        result
            .types
            .iter()
            .filter(|t| t.name == "container of (int)")
            .count(),
        1
    );
}

#[test]
fn template_defaults_fill_trailing_parameters() {
    // 8.3: trailing parameters may be omitted when defaults exist —
    // `map of (int)` binds <value'type> to its default, <key'type>.
    let result = elaborated(
        "template struct map of (<key'type>, <value'type> = <key'type>) {\n    \
         put(k: <key'type>, v: <value'type>) is undefined;\n};\n\
         extend sys {\n    m: map of (int);\n    n: map of (int, string);\n};\n",
    );
    let defaulted = find(&result, "map of (int)");
    let put = defaulted
        .methods
        .iter()
        .find(|method| method.name == "put")
        .expect("the put method");
    let render_param = |param: &Option<e1647::ast::TypeExpr>| param.as_ref().map(|ty| ty.render());
    assert_eq!(render_param(&put.params[0].ty), Some("int".to_owned()));
    assert_eq!(render_param(&put.params[1].ty), Some("int".to_owned()));
    // The explicit two-argument instance is a distinct view.
    let explicit = find(&result, "map of (int, string)");
    let put_explicit = explicit
        .methods
        .iter()
        .find(|method| method.name == "put")
        .expect("the put method");
    assert_eq!(
        render_param(&put_explicit.params[1].ty),
        Some("string".to_owned())
    );
}

#[test]
fn template_extensions_apply_to_complying_instances() {
    // 8.2 with 8.3's NOTE 3: a template extension applies to an
    // instance only if every actual complies with the types or
    // categories the extension's parameters specify. The
    // enumerated-category extension's members append (its tags may
    // differ from the template's) with the actuals substituted; the
    // scalar-parametered instance does not get it.
    let result = elaborated(
        "type color_e: [RED, GREEN];\n\
         template struct packet of (<kind'type>, <data'type>) {\n    k: <kind'type>;\n};\n\
         template extend packet of (<k'enumerated>, <d'type>) {\n    extra: <d'type>;\n    \
         describe(): uint is also { return 1; };\n};\n\
         extend sys {\n    p: packet of (color_e, int);\n    q: packet of (int, int);\n};\n",
    );
    // The enumerated actual complies: the extension's field and method
    // are on the view, with the actual substituted for the extension's
    // own formal.
    let applied = find(&result, "packet of (color_e, int)");
    assert_eq!(field_names(applied), vec!["k", "extra"]);
    assert_eq!(applied.fields[1].ty.render(), "int".to_owned());
    assert!(
        applied
            .methods
            .iter()
            .any(|method| method.name == "describe")
    );
    // The non-enumerated actual does not comply: no extension members.
    let not_applied = find(&result, "packet of (int, int)");
    assert_eq!(field_names(not_applied), vec!["k"]);
    assert!(
        not_applied
            .methods
            .iter()
            .all(|method| method.name != "describe")
    );
    // The template declaration's own composed members stay
    // extension-free — the extension applies per instance.
    let template_view = find(&result, "packet of (color_e, int)");
    assert_eq!(template_view.fields.len(), 2);
}

#[test]
fn template_extension_specific_type_parameter_form() {
    // 8.2: "the specific type where type-name is specified" — a
    // non-category spelling after the tag requires the actual to be
    // exactly that type.
    let result = elaborated(
        "type color_e: [RED, GREEN];\n\
         type width_e: [W8, W16];\n\
         template struct packet of (<kind'type>) {\n    k: <kind'type>;\n};\n\
         template extend packet of (<k'color_e>) {\n    extra: uint;\n};\n\
         extend sys {\n    p: packet of (color_e);\n    q: packet of (width_e);\n};\n",
    );
    assert_eq!(
        field_names(find(&result, "packet of (color_e)")),
        vec!["k", "extra"]
    );
    assert_eq!(field_names(find(&result, "packet of (width_e)")), vec!["k"]);
}

#[test]
fn template_extension_arity_mismatch_is_reported_once() {
    // 8.2: the extension's parameter count must equal the original
    // template declaration's — reported once, and the extension does
    // not apply.
    let parsed = parse(
        "template struct box of <type> {\n    v: <type>;\n};\n\
         template extend box of (<a'type>, <b'type>) {\n    w: <b'type>;\n};\n\
         extend sys {\n    b: box of (int);\n};\n",
    );
    assert!(parsed.diagnostics.is_empty());
    let result = elaborate(&parsed.declarations);
    let rendered: Vec<String> = result.diagnostics.iter().map(|d| d.render()).collect();
    assert_eq!(
        rendered,
        vec![
            "4:17: error[E0304]: template extension 'box' declares 2 parameter(s) but template 'box' declares 1"
        ]
    );
    // The instance still materializes without the extension's members.
    assert_eq!(field_names(find(&result, "box of (int)")), vec!["v"]);
}

#[test]
fn method_body_instance_references_substitute_and_materialize() {
    // 8.3: the substitution reaches into method bodies — the `new
    // iterator of <type>` inside a body is the instance's only
    // reference, and the worklist still materializes it with the
    // actual substituted in the body's type position.
    let result = elaborated(
        "struct base_struct {\n    id: int;\n};\n\
         template struct container of <type> like base_struct {\n    \
         make(): base_struct is {\n        return new iterator of <type> with { };\n    };\n};\n\
         template struct iterator of <type> like base_struct {\n    idx: int;\n};\n\
         extend sys {\n    c: container of (int);\n};\n",
    );
    // No field or signature references `iterator of (type>` — only the
    // method body does, and its instance materialized.
    let iterator = find(&result, "iterator of (int)");
    assert_eq!(iterator.base, Some("base_struct".to_owned()));
    // The body's `new` now names the materialized instance.
    let view = find(&result, "container of (int)");
    let make = view
        .methods
        .iter()
        .find(|method| method.name == "make")
        .expect("the make method");
    let e1647::ast::Stmt::Return {
        value: Some(e1647::ast::Expr::New {
            ty: Some(new_ty), ..
        }),
        ..
    } = &make.body[0]
    else {
        panic!("expected a returning new expression");
    };
    let e1647::ast::TypeExpr::TemplateRef { name, args, .. } = new_ty else {
        panic!("expected a template reference in the new expression");
    };
    assert_eq!(name, "iterator");
    assert_eq!(args.len(), 1);
    assert_eq!(args[0].render(), "int".to_owned());
}

#[test]
fn statement_may_span_code_segments() {
    // 4.2.3: a code segment may end in the middle of a statement (here,
    // inside a struct body between two fields); the statement completes
    // in the next code segment of the same module. The lexer skips the
    // prose between `'>` and `<'`, so the parser sees one continuous
    // member list.
    let result = elaborated(include_str!("fixtures/s2019_statement_spans_segments.e"));
    assert_eq!(field_names(find(&result, "pkt")), vec!["a", "b"]);
}

#[test]
fn brace_groups_splice_members_and_statements() {
    // 4.3.2 / 4.3.3: bare `{ ... }` groups at module level and in member
    // lists are pure syntax — no scope, no semantics. Members splice
    // into the enclosing list in source order, groups nest, and an
    // `#ifdef` inside a group keeps its member-position meaning.
    let result = elaborated(include_str!("fixtures/s2019_brace_groups.e"));
    let pkt = find(&result, "pkt");
    assert_eq!(field_names(pkt), vec!["a", "b", "c"]);
    assert_eq!(pkt.constraints.len(), 1, "the grouped `keep` spliced in");
    let color = find(&result, "color_t");
    let enumeral_names: Vec<&str> = color
        .enumerals
        .iter()
        .map(|enumeral| enumeral.name.as_str())
        .collect();
    assert_eq!(enumeral_names, vec!["RED", "GREEN"]);
    // A nested group and an `#ifdef` whose body sits inside a group.
    let result = elaborated(
        "<'\n#define X\nstruct s {\n    { { x: uint; } }\n    {\n        \
         #ifdef X { y: uint; }\n    }\n    z: uint;\n};\n'>\n",
    );
    assert_eq!(field_names(find(&result, "s")), vec!["x", "y", "z"]);
}

#[test]
fn typeof_type_positions_resolve_in_elaboration() {
    // 5.9: `typeof_item(data)` resolves to the list's element type and
    // `typeof(first_item)` chains to the same static type — the
    // elaborated fields carry the resolved types (`byte`), the
    // placeholders rewritten in place.
    let result = elaborated(
        "struct s {\n    data: list of byte;\n    first_item: typeof_item(data);\n    n: typeof(first_item);\n};\n",
    );
    let s = find(&result, "s");
    for field in &s.fields {
        assert_eq!(
            field.ty.render(),
            match field.name.as_str() {
                "data" => "list of byte",
                "first_item" => "byte",
                "n" => "byte",
                other => panic!("unexpected field '{other}'"),
            },
            "field '{}' resolves to '{}'",
            field.name,
            field.ty.render()
        );
    }
}

#[test]
fn typeof_resolves_method_signature_and_local_var_positions() {
    // 5.9: the resolution covers method parameters, return types and
    // local `var` declarations, in declaration order — `wrap()`'s
    // parameter and locals type against the struct's resolved fields.
    let result = elaborated(
        "struct s {\n    data: list of byte;\n    first_item: typeof_item(data);\n    \
         wrap(v: typeof(first_item)): typeof(first_item) is {\n        \
         var k: typeof(v);\n        k = v + 1;\n        return k;\n    };\n};\n",
    );
    let s = find(&result, "s");
    let wrap = s
        .methods
        .iter()
        .find(|method| method.name == "wrap")
        .expect("method 'wrap' elaborated");
    let param_ty = wrap.params[0].ty.as_ref().expect("typed parameter");
    assert_eq!(param_ty.render(), "byte");
    assert_eq!(
        wrap.return_type.as_ref().expect("return type").render(),
        "byte"
    );
    let e1647::ast::Stmt::VarDecl {
        ty: Some(local), ..
    } = &wrap.body[0]
    else {
        panic!("expected a typed local declaration");
    };
    assert_eq!(local.render(), "byte");
}

#[test]
fn bare_when_words_resolve_to_their_determinants() {
    // 6.7 with 4.17.3: a when header's bare value words name their
    // determinant fields — `when red good packet` fixes `color == red`
    // and `good == TRUE`, and the synthesized view is named with the
    // written words (`red good packet`), which the subtype-qualified
    // access resolves against.
    let result = elaborated(
        "type color_t: [red, green, blue];\n\
         struct packet {\n    color: color_t;\n    good: bool;\n    \
         when red good packet { x: int; keep x == 5; };\n};\n",
    );
    let view = find(&result, "red good packet");
    assert_eq!(
        view.when_of,
        vec![
            ("color".to_owned(), "red".to_owned()),
            ("good".to_owned(), "TRUE".to_owned())
        ]
    );
    assert_eq!(view.base.as_deref(), Some("packet"));
    assert_eq!(field_names(view), vec!["color", "good", "x"]);
    // The chained full-form header keeps its own reading.
    let result = elaborated(
        "type color_t: [red, green, blue];\n\
         type size_t: [big, medium, small];\n\
         struct packet {\n    color: color_t;\n    size: size_t;\n    \
         when big'size green'color packet { y: uint; };\n};\n",
    );
    let view = find(&result, "big green packet");
    assert_eq!(
        view.when_of,
        vec![
            ("size".to_owned(), "big".to_owned()),
            ("color".to_owned(), "green".to_owned())
        ]
    );
}

#[test]
fn spelled_when_types_materialize_as_views() {
    // 6.7: a when-qualified type spelling without a when block (`good
    // packet`) still names a subtype view — the root base's members
    // under the spelling's name, with the spelling's words resolved as
    // determinants (`good` is the Boolean field's short form, so the
    // view fixes `good == TRUE`).
    let result = elaborated(
        "type color_t: [red, green, blue];\n\
         struct packet {\n    color: color_t;\n    good: bool;\n    \
         when red good packet { x: int; };\n};\n\
         extend sys {\n    p1: good packet;\n};\n",
    );
    let view = find(&result, "good packet");
    assert_eq!(view.when_of, vec![("good".to_owned(), "TRUE".to_owned())]);
    assert_eq!(view.base.as_deref(), Some("packet"));
    assert_eq!(field_names(view), vec!["color", "good"]);
    assert!(!view.fields.iter().any(|f| f.name == "x"));
}

#[test]
fn unit_attribute_constraints_record_with_their_field() {
    // T8.4 (7.3.4, 7.3.5): `keep <field>.pack_options() == …` and
    // `keep <field>.per_inst_cover() == …` record on the declaring
    // type like the port hdl paths do — (field, attribute, value as
    // written) — and stay out of gen (the attribute names are not
    // fields, and the values are predefined settings).
    let result = elaborated(
        "unit u {\n};\n\
         extend sys {\n    u1: u;\n    \
         keep u1.pack_options() == packing.low_big_endian;\n    \
         keep u1.per_inst_cover() == DEFAULT;\n};\n",
    );
    let sys = find(&result, "sys");
    assert_eq!(
        sys.unit_attributes,
        vec![
            (
                "u1".to_owned(),
                "pack_options".to_owned(),
                "packing.low_big_endian".to_owned()
            ),
            (
                "u1".to_owned(),
                "per_inst_cover".to_owned(),
                "DEFAULT".to_owned()
            ),
        ]
    );
}
