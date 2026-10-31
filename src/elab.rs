//! Member composition for the declared subset: `like` inheritance chains
//! and `extend` blocks are folded into one composed member list per
//! `struct`/`unit`, and `type` declarations pass through with their
//! enumerals.
//!
//! This is **not** full elaboration: `when` subtypes are carried, not
//! resolved; there is no type checking and no execution semantics.
//! Composition is order-independent on declarations (e allows an `extend`
//! before its target) and deterministic: base-most `like` members first,
//! then the declaring members, then each `extend` block in source order.
//! The built-in `sys` unit is extended without being declared and is
//! synthesized from its `extend` blocks alone.
//!
//! ## Diagnostics (on top of the parser's)
//!
//! - `E0301` `like` inheritance cycle (reported once per type)
//! - `E0302` the same field name is contributed by two different
//!   declarations or extend blocks (duplicates inside one declaration are
//!   the parser's `E0202`; reported once per name)
//! - `E0303` a method is redefined without `is also` / `is first` /
//!   `is only` (reported once per name)

use crate::ast::{
    Declaration, DeclarationKind, Enumeral, Field, KeepConstraint, Member, MethodDecl, WhenDecl,
};
use crate::diagnostic::Diagnostic;
use std::collections::{BTreeMap, BTreeSet};

/// One elaborated type: a `struct`/`unit` with its composed member lists,
/// or a `type` declaration with its enumerals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElaboratedType {
    pub kind: DeclarationKind,
    pub name: String,
    /// The `like` base, as written.
    pub base: Option<String>,
    pub fields: Vec<Field>,
    pub constraints: Vec<KeepConstraint>,
    pub methods: Vec<MethodDecl>,
    /// `when` subtype members, ancestor types first.
    pub whens: Vec<WhenDecl>,
    /// `on` struct members (clause 11.2), base types first, then extends
    /// in declaration order — the accumulated action blocks per event.
    pub ons: Vec<crate::ast::OnDecl>,
    /// Coverage groups (clause 14), recorded not collected.
    pub covers: Vec<crate::ast::CoverGroupDecl>,
    /// `expect`/`assume` rules (clause 11.4), base types first, then
    /// extends in declaration order — monitored over the run log by the
    /// interpreter.
    pub expects: Vec<crate::ast::ExpectDecl>,
    /// `type name: <type>;` — the underlying representation, recorded
    /// only (clause 5); distinct-type semantics are a later milestone.
    pub underlying: Option<crate::ast::TypeExpr>,
    pub enumerals: Vec<Enumeral>,
    /// `simple_port` members with the HDL location resolved from their
    /// `keep name.hdl_path() == "path"` constraints.
    pub ports: Vec<PortInfo>,
    pub span: crate::ast::Span,
}

/// A `simple_port` member resolved for simulation: the port name and the
/// HDL location its `hdl_path()` constraint declared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortInfo {
    pub name: String,
    pub hdl_path: Option<String>,
}

/// One `sequence NAME using item = T, created_driver = D, created_kind
/// = K;` declaration, recovered from the synthesized define markers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceInfo {
    pub name: String,
    pub item: String,
    pub driver: String,
    pub kind: String,
}

/// Outcome of elaborating one file's declarations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Elaborated {
    /// In declaration order; `extend` blocks are folded into their targets
    /// and do not appear separately.
    pub types: Vec<ElaboratedType>,
    /// `define` constants by name (literal values only).
    pub defines: BTreeMap<String, crate::ast::Expr>,
    /// Sequence declarations recovered from the parser's synthesized
    /// markers.
    pub sequences: Vec<SequenceInfo>,
    pub diagnostics: Vec<Diagnostic>,
}

/// One type's members, tagged with the declaration or extend block that
/// contributed them (its index in the declaration slice).
#[derive(Debug, Default)]
struct Composed<'a> {
    fields: Vec<(&'a Field, usize)>,
    constraints: Vec<&'a KeepConstraint>,
    methods: Vec<(&'a MethodDecl, usize)>,
    whens: Vec<&'a WhenDecl>,
    ons: Vec<&'a crate::ast::OnDecl>,
    covers: Vec<&'a crate::ast::CoverGroupDecl>,
    expects: Vec<&'a crate::ast::ExpectDecl>,
    ports: Vec<(&'a crate::ast::PortDecl, usize)>,
}

struct Elaborator<'a> {
    declarations: &'a [Declaration],
    base_by_name: BTreeMap<&'a str, usize>,
    extends_by_target: BTreeMap<&'a str, Vec<usize>>,
    diagnostics: Vec<Diagnostic>,
    reported_fields: BTreeSet<&'a str>,
    reported_methods: BTreeSet<&'a str>,
    reported_cycles: BTreeSet<&'a str>,
}

/// Fold `like` chains and `extend` blocks into per-type member lists.
pub fn elaborate(declarations: &[Declaration]) -> Elaborated {
    let mut base_by_name: BTreeMap<&str, usize> = BTreeMap::new();
    let mut extends_by_target: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, declaration) in declarations.iter().enumerate() {
        match declaration.kind {
            DeclarationKind::Extend => {
                extends_by_target
                    .entry(declaration.name.as_str())
                    .or_default()
                    .push(index);
            }
            DeclarationKind::Struct | DeclarationKind::Unit | DeclarationKind::Type => {
                base_by_name.insert(declaration.name.as_str(), index);
            }
        }
    }
    let mut elaborator = Elaborator {
        declarations,
        base_by_name,
        extends_by_target,
        diagnostics: Vec::new(),
        reported_fields: BTreeSet::new(),
        reported_methods: BTreeSet::new(),
        reported_cycles: BTreeSet::new(),
    };
    let mut types = Vec::new();
    for declaration in declarations {
        match declaration.kind {
            DeclarationKind::Struct | DeclarationKind::Unit => {
                let mut stack = Vec::new();
                let composed = elaborator.compose(declaration.name.as_str(), &mut stack);
                let port_paths = elaborator.extract_port_paths(declaration);
                types.push(ElaboratedType {
                    kind: declaration.kind,
                    name: declaration.name.clone(),
                    base: declaration.like.clone(),
                    fields: composed
                        .fields
                        .into_iter()
                        .map(|(field, _)| field.clone())
                        .collect(),
                    constraints: composed.constraints.into_iter().cloned().collect(),
                    methods: composed
                        .methods
                        .into_iter()
                        .map(|(method, _)| method.clone())
                        .collect(),
                    whens: composed.whens.into_iter().cloned().collect(),
                    ons: composed.ons.into_iter().cloned().collect(),
                    covers: composed.covers.into_iter().cloned().collect(),
                    expects: composed.expects.into_iter().cloned().collect(),
                    underlying: None,
                    enumerals: Vec::new(),
                    ports: composed
                        .ports
                        .into_iter()
                        .map(|(port, _)| PortInfo {
                            name: port.name.clone(),
                            hdl_path: port_paths.get(port.name.as_str()).cloned(),
                        })
                        .collect(),
                    span: declaration.span,
                });
            }
            DeclarationKind::Type => types.push(ElaboratedType {
                kind: DeclarationKind::Type,
                name: declaration.name.clone(),
                base: None,
                fields: Vec::new(),
                constraints: Vec::new(),
                methods: Vec::new(),
                whens: Vec::new(),
                ons: Vec::new(),
                covers: Vec::new(),
                expects: Vec::new(),
                underlying: declaration.underlying.clone(),
                ports: Vec::new(),
                enumerals: declaration
                    .members
                    .iter()
                    .filter_map(|member| match member {
                        Member::Enumeral(enumeral) => Some(enumeral.clone()),
                        _ => None,
                    })
                    .collect(),
                span: declaration.span,
            }),
            DeclarationKind::Extend => {}
        }
    }
    elaborator.compose_builtin_units(&mut types);
    let mut defines = BTreeMap::new();
    for declaration in declarations {
        for member in &declaration.members {
            if let Member::Define { name, value, .. } = member
                && let Some(value) = value
            {
                defines.insert(name.clone(), value.clone());
            }
        }
    }
    // Recover sequence declarations from the parser's synthesized
    // `<name>.item` / `<name>.created_driver` / `<name>.created_kind`
    // string defines on the sequence struct.
    let mut sequences = Vec::new();
    for declaration in declarations {
        let prefix = format!("{}.", declaration.name);
        let mut item = None;
        let mut driver = None;
        let mut kind = None;
        for member in &declaration.members {
            let Member::Define { name, value, .. } = member else {
                continue;
            };
            let Some(field) = name.strip_prefix(&prefix) else {
                continue;
            };
            let Some(crate::ast::Expr::StringLiteral { value, .. }) = value else {
                continue;
            };
            match field {
                "item" => item = Some(value.clone()),
                "created_driver" => driver = Some(value.clone()),
                "created_kind" => kind = Some(value.clone()),
                _ => {}
            }
        }
        if let (Some(item), Some(driver)) = (item, driver) {
            sequences.push(SequenceInfo {
                name: declaration.name.clone(),
                item,
                driver,
                kind: kind.unwrap_or_else(|| format!("{}_kind_t", declaration.name)),
            });
        }
    }
    Elaborated {
        types,
        defines,
        sequences,
        diagnostics: elaborator.diagnostics,
    }
}

impl<'a> Elaborator<'a> {
    /// A built-in unit (`sys`) is extended without being declared: fold its
    /// extend blocks into a synthesized type appended after the declared
    /// types.
    fn compose_builtin_units(&mut self, types: &mut Vec<ElaboratedType>) {
        for &builtin in crate::builtins::BUILTIN_UNITS {
            if self.base_by_name.contains_key(builtin) {
                continue;
            }
            let Some(extend_indices) = self.extends_by_target.get(builtin).cloned() else {
                continue;
            };
            let mut composed = Composed::default();
            let mut port_paths: BTreeMap<String, String> = BTreeMap::new();
            for extend_index in &extend_indices {
                let extend = &self.declarations[*extend_index];
                Self::push_members(extend, *extend_index, &mut composed);
                port_paths.extend(self.extract_port_paths(extend));
            }
            self.check_duplicate_fields(builtin, &composed);
            self.check_method_redefinitions(builtin, &composed);
            types.push(ElaboratedType {
                kind: DeclarationKind::Unit,
                name: builtin.to_owned(),
                base: None,
                fields: composed
                    .fields
                    .into_iter()
                    .map(|(field, _)| field.clone())
                    .collect(),
                constraints: composed.constraints.into_iter().cloned().collect(),
                methods: composed
                    .methods
                    .into_iter()
                    .map(|(method, _)| method.clone())
                    .collect(),
                whens: composed.whens.into_iter().cloned().collect(),
                ons: composed.ons.into_iter().cloned().collect(),
                covers: composed.covers.into_iter().cloned().collect(),
                expects: composed.expects.into_iter().cloned().collect(),
                underlying: None,
                enumerals: Vec::new(),
                ports: composed
                    .ports
                    .into_iter()
                    .map(|(port, _)| PortInfo {
                        name: port.name.clone(),
                        hdl_path: port_paths.get(port.name.as_str()).cloned(),
                    })
                    .collect(),
                span: self.declarations[extend_indices[0]].span,
            });
        }
    }

    /// Resolve `keep <port>.hdl_path() == "path"` constraints of one
    /// declaration into a port-name → HDL-location map.
    fn extract_port_paths(&self, declaration: &'a Declaration) -> BTreeMap<String, String> {
        use crate::ast::{BinaryOp, Expr};
        let port_names: Vec<&str> = declaration
            .members
            .iter()
            .filter_map(|member| match member {
                Member::Port(port) => Some(port.name.as_str()),
                _ => None,
            })
            .collect();
        if port_names.is_empty() {
            return BTreeMap::new();
        }
        let mut paths = BTreeMap::new();
        for member in &declaration.members {
            let Member::Constraint(constraint) = member else {
                continue;
            };
            let Expr::Binary {
                op: BinaryOp::Eq,
                lhs,
                rhs,
                ..
            } = &constraint.expr
            else {
                continue;
            };
            // Either orientation of `<port>.hdl_path() == "<path>"`.
            for (call, literal) in [(lhs.as_ref(), rhs.as_ref()), (rhs.as_ref(), lhs.as_ref())] {
                let Expr::Call { callee, args, .. } = call else {
                    continue;
                };
                if !args.is_empty() {
                    continue;
                }
                let Expr::MemberAccess {
                    object,
                    name: method,
                    ..
                } = callee.as_ref()
                else {
                    continue;
                };
                let Expr::Ident { name: port, .. } = object.as_ref() else {
                    continue;
                };
                if method != "hdl_path" || !port_names.contains(&port.as_str()) {
                    continue;
                }
                if let Expr::StringLiteral { value, .. } = literal {
                    paths.insert(port.to_string(), value.clone());
                }
            }
        }
        paths
    }

    fn compose(&mut self, name: &'a str, stack: &mut Vec<&'a str>) -> Composed<'a> {
        let Some(&index) = self.base_by_name.get(name) else {
            return Composed::default();
        };
        let declaration: &'a Declaration = &self.declarations[index];
        if declaration.kind == DeclarationKind::Type {
            // A `like` target that is an enumeral type contributes nothing;
            // full type checking is a later milestone.
            return Composed::default();
        }
        if stack.contains(&name) {
            if self.reported_cycles.insert(name) {
                let (start, end) = declaration.like_span.unwrap_or(declaration.name_span);
                self.diagnostics.push(Diagnostic::new(
                    "E0301",
                    format!("inheritance cycle involving '{name}'"),
                    start,
                    end,
                ));
            }
            return Composed::default();
        }
        stack.push(name);
        let mut composed = match &declaration.like {
            Some(base) => self.compose(base.as_str(), stack),
            None => Composed::default(),
        };
        Self::push_members(declaration, index, &mut composed);
        let extend_indices = self
            .extends_by_target
            .get(name)
            .cloned()
            .unwrap_or_default();
        for extend_index in extend_indices {
            let extend = &self.declarations[extend_index];
            Self::push_members(extend, extend_index, &mut composed);
        }
        self.check_duplicate_fields(name, &composed);
        self.check_method_redefinitions(name, &composed);
        stack.pop();
        composed
    }

    fn push_members(declaration: &'a Declaration, block: usize, composed: &mut Composed<'a>) {
        for member in &declaration.members {
            match member {
                Member::Field(field) => composed.fields.push((field, block)),
                Member::Constraint(constraint) => composed.constraints.push(constraint),
                Member::Method(method) => composed.methods.push((method, block)),
                Member::When(when) => composed.whens.push(when),
                Member::On(on) => composed.ons.push(on),
                Member::Cover(cover) => composed.covers.push(cover),
                Member::Expect(expect) => composed.expects.push(expect),
                Member::Port(port) => composed.ports.push((port, block)),
                Member::Import { .. } | Member::Define { .. } | Member::Macro(_) => {}
                Member::Event(_) | Member::Enumeral(_) => {}
            }
        }
    }

    /// One diagnostic per field name that two different blocks contribute;
    /// duplicates inside one block are the parser's `E0202`.
    fn check_duplicate_fields(&mut self, type_name: &'a str, composed: &Composed<'a>) {
        for i in 0..composed.fields.len() {
            for j in i + 1..composed.fields.len() {
                let (field_i, block_i) = &composed.fields[i];
                let (field_j, block_j) = &composed.fields[j];
                if field_i.name == field_j.name
                    && block_i != block_j
                    && self.reported_fields.insert(field_j.name.as_str())
                {
                    self.diagnostics.push(Diagnostic::new(
                        "E0302",
                        format!(
                            "duplicate field '{}' in elaborated type '{}'",
                            field_j.name, type_name
                        ),
                        field_j.name_span.0,
                        field_j.name_span.1,
                    ));
                }
            }
        }
    }

    /// One diagnostic per method name that is redefined without an
    /// extension modifier.
    fn check_method_redefinitions(&mut self, type_name: &'a str, composed: &Composed<'a>) {
        for i in 0..composed.methods.len() {
            let (method, _) = &composed.methods[i];
            if method.modifier.is_some() {
                continue;
            }
            let redefined = composed.methods[..i]
                .iter()
                .any(|(earlier, _)| earlier.name == method.name);
            if redefined && self.reported_methods.insert(method.name.as_str()) {
                self.diagnostics.push(Diagnostic::new(
                    "E0303",
                    format!(
                        "method '{}' is redefined in elaborated type '{}' without 'is also', 'is first' or 'is only'",
                        method.name, type_name
                    ),
                    method.name_span.0,
                    method.name_span.1,
                ));
            }
        }
    }
}
