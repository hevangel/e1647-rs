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
