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
    // 6.6: a single-modifier `when VALUE'id` block synthesizes a
    // subtype view named `VALUE base` — the base's composed members
    // with the when block's appended — marked with its discriminator.
    let result = elaborated(
        "type mode_t : [LO, HI];\n\
         struct pkt {\n    kind: mode_t;\n    v: uint (bits: 8);\n    \
         when LO kind is {\n            keep soft v == select { 100 : min; };\n        };\n    \
         when HI kind is {\n            keep soft v == select { 100 : max; };\n        };\n};\n",
    );
    let view = find(&result, "LO pkt");
    assert_eq!(view.when_of, Some(("kind".to_owned(), "LO".to_owned())));
    assert_eq!(view.base, Some("pkt".to_owned()));
    // The view carries the base's fields plus the when constraint.
    assert_eq!(view.fields.len(), 2);
    assert_eq!(view.constraints.len(), 1);
    let hi = find(&result, "HI pkt");
    assert_eq!(hi.when_of, Some(("kind".to_owned(), "HI".to_owned())));
    assert_eq!(hi.constraints.len(), 1);
    // The base itself stays unmarked and without the when constraints.
    let base = find(&result, "pkt");
    assert_eq!(base.when_of, None);
    assert_eq!(base.constraints.len(), 0);
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
