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
    Declaration, DeclarationKind, Enumeral, Expr, Field, KeepConstraint, Member, MethodDecl, Stmt,
    WhenDecl,
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
    /// For a synthesized when-subtype view (6.6): every discriminating
    /// (field id, modifier value) pair — `[("kind", "BUSY")]` for
    /// `when BUSY'kind`, `[("kind", "LO"), ("sel", "HI")]` for the
    /// chained `when LO'kind HI'sel` or the equivalent two-level
    /// when-in-when composition. Empty for regular types.
    pub when_of: Vec<(String, String)>,
    /// `event` members (11.1.3) with their triggers as written — the
    /// simulator-side triggers (`rise/fall/change(HDL expression) @sim`,
    /// 11.1.4.2) drive the interpreter's sampling. Every instance owns
    /// its events (11.1.2); the interpreter instantiates per instance.
    pub events: Vec<crate::ast::EventDecl>,
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
    /// An event port (9.4): carries events, not data — its `$` event is
    /// triggerable by sampling the bound HDL object per its `edge()`
    /// attribute (9.7.2.8), not only by `emit port$`.
    pub is_event: bool,
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
    events: Vec<&'a crate::ast::EventDecl>,
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
    let mut template_extends_by_target: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, declaration) in declarations.iter().enumerate() {
        match declaration.kind {
            DeclarationKind::Extend => {
                // A template extension (8.2) applies per instance at
                // monomorphization (8.3's NOTE 3) — the template's own
                // composed members stay extension-free, and a
                // non-matching instance does not get the extension.
                if declaration.template_extend {
                    template_extends_by_target
                        .entry(declaration.name.as_str())
                        .or_default()
                        .push(index);
                } else {
                    extends_by_target
                        .entry(declaration.name.as_str())
                        .or_default()
                        .push(index);
                }
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
                    when_of: Vec::new(),
                    events: composed.events.into_iter().cloned().collect(),
                    underlying: None,
                    enumerals: Vec::new(),
                    ports: composed
                        .ports
                        .into_iter()
                        .map(|(port, _)| PortInfo {
                            name: port.name.clone(),
                            hdl_path: port_paths.get(port.name.as_str()).cloned(),
                            is_event: matches!(
                                &port.element,
                                crate::ast::TypeExpr::Named { name, .. } if name == "event"
                            ),
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
                when_of: Vec::new(),
                events: Vec::new(),
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
    // 6.6: synthesize one subtype view per `when` block (`when
    // BUSY'kind { .. }` on type `kind`'s owner): the base's composed
    // members with the when block's members appended, under the
    // `VALUE base` name the field-type spelling uses. Instances typed
    // `VALUE base` (TypeExpr::WhenQualified) carry this view, so the
    // when block's fields, constraints, methods, on blocks and cover
    // groups apply for them.
    //
    // Chained modifiers compose conjunctively (6.6.1: subtypes can be
    // combinations of fields): every modifier names its determinant
    // field, and the view carries all the (field, value) pairs under a
    // name that space-joins the values in modifier order (`V1 V2
    // base`). A `when` member nested inside a when block applies on
    // top of the enclosing view: the outer view is synthesized first
    // and the inner when's modifiers run against the synthesized
    // view's fields, producing a further view named with the combined
    // values. Both levels share one worklist: each synthesized view is
    // queued back, and its own nested whens synthesize on top of it
    // until no new views appear (budget-capped — 200 synthesized
    // views — against pathological nesting).
    let base_count = types.len();
    let mut worklist: Vec<usize> = (0..base_count).collect();
    let mut budget = 200usize;
    while let Some(index) = worklist.pop() {
        let whens: Vec<crate::ast::WhenDecl> = types[index].whens.clone();
        for when in whens {
            let Some(pairs) = when_modifier_pairs(&when) else {
                continue;
            };
            if budget == 0 {
                break;
            }
            budget -= 1;
            // The enclosing view's own values (none for a base type)
            // come first, the inner when's values follow, and the
            // root base type's name closes — the reading order the
            // chained field-type spelling uses (`f: LO HI pkt`).
            let mut values: Vec<String> = types[index]
                .when_of
                .iter()
                .map(|(_, value)| value.clone())
                .collect();
            values.extend(pairs.iter().map(|(_, value)| value.clone()));
            let base_name = when_root_base_name(&types, index);
            let enclosing_name = types[index].name.clone();
            let mut view = types[index].clone();
            view.name = format!("{} {}", values.join(" "), base_name);
            // The combined view's direct base is the view it applies on
            // top of (`LO HI pkt` → `LO pkt` → `pkt`), so `is a` walks
            // the containment chain.
            view.base = Some(enclosing_name);
            let mut all_pairs = types[index].when_of.clone();
            all_pairs.extend(pairs);
            view.when_of = all_pairs;
            // The base's other when blocks are alternative subtypes,
            // not refinements of this one — the view composes only the
            // when-in-when members of the block it came from.
            view.whens = Vec::new();
            for member in &when.members {
                match member {
                    crate::ast::Member::Field(field) => view.fields.push(field.clone()),
                    crate::ast::Member::Constraint(constraint) => {
                        view.constraints.push(constraint.clone())
                    }
                    crate::ast::Member::Method(method) => view.methods.push(method.clone()),
                    crate::ast::Member::On(on) => view.ons.push(on.clone()),
                    crate::ast::Member::Cover(cover) => view.covers.push(cover.clone()),
                    crate::ast::Member::Expect(expect) => view.expects.push(expect.clone()),
                    // A when-in-when member rides on the synthesized
                    // view: carry it in the view's whens so the worklist
                    // applies it on top.
                    crate::ast::Member::When(inner_when) => view.whens.push(inner_when.clone()),
                    _ => {}
                }
            }
            types.push(view);
            worklist.push(types.len() - 1);
        }
    }
    // 8.3: monomorphize the template instances the code uses. "A
    // template instance creates a new struct, by substituting an
    // actual parameter for the corresponding template parameter within
    // the template body" — and within the parameterized base and the
    // bodies of any applicable template extensions (8.2). Trailing
    // parameters may be omitted when defaults exist; two instances
    // with the same actuals are the same instance (the canonical
    // name). The worklist seeds from every type's member types and
    // method bodies, and grows with the references substitution
    // uncovers.
    let mut queue: Vec<(String, Vec<crate::ast::TypeExpr>)> = Vec::new();
    for entry in &types {
        enqueue_member_refs(entry, &mut queue);
    }
    let mut done: BTreeSet<String> = types.iter().map(|t| t.name.clone()).collect();
    let mut reported_extension_arity: BTreeSet<usize> = BTreeSet::new();
    let mut budget = 500;
    while let Some((template_name, args)) = queue.pop() {
        budget -= 1;
        if budget == 0 {
            break;
        }
        let canonical = crate::ast::template_instance_name(&template_name, &args);
        if done.contains(&canonical) {
            continue;
        }
        done.insert(canonical.clone());
        let Some(template) = declarations.iter().find(|declaration| {
            declaration.name == template_name && !declaration.template_params.is_empty()
        }) else {
            continue;
        };
        // Bind formals: positional actuals, then defaults (8.3's
        // trailing-omission rule). A parameter with neither is not
        // provided for — the instance is skipped, honestly. The full
        // positional actual set (defaults filled) also drives the
        // 8.3-NOTE-3 extension compliance below.
        let mut bindings: BTreeMap<String, crate::ast::TypeExpr> = BTreeMap::new();
        let mut actuals: Vec<crate::ast::TypeExpr> = Vec::new();
        let mut satisfied = true;
        for (index, param) in template.template_params.iter().enumerate() {
            if let Some(actual) = args.get(index) {
                bindings.insert(param.name.clone(), actual.clone());
                actuals.push(actual.clone());
            } else if let Some(default) = &param.default {
                // A default may reference earlier formals
                // (`<value'type> = <key'type>`): substitute it through
                // the bindings built so far.
                let substituted = substitute_type(default, &bindings, &mut queue);
                bindings.insert(param.name.clone(), substituted.clone());
                actuals.push(substituted);
            } else {
                satisfied = false;
                break;
            }
        }
        if !satisfied || bindings.is_empty() {
            continue;
        }
        let mut stack = Vec::new();
        let composed = elaborator.compose(template.name.as_str(), &mut stack);
        let mut fields = Vec::new();
        for (field, _) in composed.fields {
            let mut owned = field.clone();
            owned.ty = substitute_type(&owned.ty, &bindings, &mut queue);
            fields.push(owned);
        }
        let mut constraints: Vec<KeepConstraint> =
            composed.constraints.into_iter().cloned().collect();
        for constraint in &mut constraints {
            substitute_expr(&mut constraint.expr, &bindings, &mut queue);
        }
        let mut methods = Vec::new();
        for (method, _) in composed.methods {
            methods.push(substitute_method(method.clone(), &bindings, &mut queue));
        }
        let mut ons: Vec<crate::ast::OnDecl> = composed.ons.into_iter().cloned().collect();
        for on in &mut ons {
            substitute_statements(&mut on.body, &bindings, &mut queue);
        }
        // 8.2 with 8.3's NOTE 3: each template extension applies to
        // this instance only if all actual parameters comply with the
        // types or type categories specified for the corresponding
        // parameters in that extension. A complying extension's
        // members substitute the extension's own formals (its tags may
        // differ from the template's, 8.2) with the instance's actuals
        // and append; a non-complying one does not apply at all.
        let extension_indices = template_extends_by_target
            .get(template.name.as_str())
            .cloned()
            .unwrap_or_default();
        for extension_index in extension_indices {
            let extension = &declarations[extension_index];
            if extension.template_params.len() != template.template_params.len() {
                // 8.2: the extension's parameter count must equal the
                // original template declaration's — reported once per
                // extension, and the extension does not apply.
                if reported_extension_arity.insert(extension_index) {
                    let (start, end) = extension.name_span;
                    elaborator.diagnostics.push(Diagnostic::new(
                        "E0304",
                        format!(
                            "template extension '{}' declares {} parameter(s) but template '{}' declares {}",
                            extension.name,
                            extension.template_params.len(),
                            template.name,
                            template.template_params.len()
                        ),
                        start,
                        end,
                    ));
                }
                continue;
            }
            if !extension_applies(extension, &actuals, &types) {
                continue;
            }
            let mut extension_bindings: BTreeMap<String, crate::ast::TypeExpr> = BTreeMap::new();
            for (param, actual) in extension.template_params.iter().zip(actuals.iter()) {
                extension_bindings.insert(param.name.clone(), actual.clone());
            }
            for member in &extension.members {
                match member {
                    Member::Field(field) => {
                        let mut owned = field.clone();
                        owned.ty = substitute_type(&owned.ty, &extension_bindings, &mut queue);
                        fields.push(owned);
                    }
                    Member::Constraint(constraint) => {
                        let mut owned = constraint.clone();
                        substitute_expr(&mut owned.expr, &extension_bindings, &mut queue);
                        constraints.push(owned);
                    }
                    Member::Method(method) => {
                        methods.push(substitute_method(
                            method.clone(),
                            &extension_bindings,
                            &mut queue,
                        ));
                    }
                    Member::On(on) => {
                        let mut owned = on.clone();
                        substitute_statements(&mut owned.body, &extension_bindings, &mut queue);
                        ons.push(owned);
                    }
                    // A `when` member of an extension is clause 8.4's
                    // template subtype instance; cover groups, expects,
                    // ports, events, defines, imports and macros in an
                    // extension body do not compose yet — documented gaps.
                    _ => {}
                }
            }
        }
        // The parameterized base substitutes to this view's base name —
        // a plain type's name, or another instance's canonical name
        // (enqueued above, materialized by the worklist).
        let base = template.like_type.as_ref().map(|like_type| {
            match substitute_type(like_type, &bindings, &mut queue) {
                crate::ast::TypeExpr::TemplateRef { name, args, .. } => {
                    crate::ast::template_instance_name(&name, &args)
                }
                crate::ast::TypeExpr::Named { name, .. } => name,
                other => other.render(),
            }
        });
        types.push(ElaboratedType {
            kind: template.kind,
            name: canonical,
            base,
            fields,
            constraints,
            methods,
            whens: composed.whens.into_iter().cloned().collect(),
            ons,
            covers: composed.covers.into_iter().cloned().collect(),
            expects: composed.expects.into_iter().cloned().collect(),
            when_of: Vec::new(),
            events: Vec::new(),
            underlying: None,
            enumerals: Vec::new(),
            ports: Vec::new(),
            span: template.span,
        });
    }
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
                when_of: Vec::new(),
                events: composed.events.into_iter().cloned().collect(),
                underlying: None,
                enumerals: Vec::new(),
                ports: composed
                    .ports
                    .into_iter()
                    .map(|(port, _)| PortInfo {
                        name: port.name.clone(),
                        hdl_path: port_paths.get(port.name.as_str()).cloned(),
                        is_event: matches!(
                            &port.element,
                            crate::ast::TypeExpr::Named { name, .. } if name == "event"
                        ),
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
                Member::Event(event) => composed.events.push(event),
                Member::Import { .. } | Member::Define { .. } | Member::Macro(_) => {}
                Member::Enumeral(_) => {}
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

/// The unmarked base type a synthesized when view (or chain of views)
/// was built from: the enclosing view's own name for a base type, the
/// innermost `base` otherwise — every when view's base chain ends at a
/// type whose [`ElaboratedType::when_of`] is empty.
fn when_root_base_name(types: &[ElaboratedType], index: usize) -> String {
    let mut cursor = index;
    while !types[cursor].when_of.is_empty() {
        let base = types[cursor]
            .base
            .as_deref()
            .expect("a synthesized when view always carries a base");
        cursor = types
            .iter()
            .position(|ty| ty.name == base)
            .expect("the view's base type is elaborated");
    }
    types[cursor].name.clone()
}

/// The (field id, value) pairs a when block's modifiers declare, or
/// `None` when the modifier spelling does not identify its determinant
/// fields. Two spellings carry the discriminators (6.6.1 — subtypes can
/// be combinations of fields): the conventional `value'id` modifiers,
/// any number of them (`when BUSY'kind`, `when LO'kind HI'dir`), and
/// the corpus's space form `when NO_CONSTRAINT env` (two bare modifiers
/// — value first, field id second). A lone bare modifier names no
/// field and synthesizes nothing.
fn when_modifier_pairs(when: &WhenDecl) -> Option<Vec<(String, String)>> {
    match when.modifiers.as_slice() {
        [] => None,
        modifiers if modifiers.iter().all(|modifier| modifier.id.is_some()) => Some(
            modifiers
                .iter()
                .map(|modifier| {
                    let (id, _) = modifier.id.as_ref().expect("checked above");
                    (id.clone(), modifier.value.clone())
                })
                .collect(),
        ),
        [first, second] if first.id.is_none() && second.id.is_none() => {
            Some(vec![(second.value.clone(), first.value.clone())])
        }
        _ => None,
    }
}

/// Substitute the bound actuals for the formals of a type position
/// (8.3), enqueueing every template reference the substitution
/// uncovers so the worklist materializes their instances.
fn substitute_type(
    ty: &crate::ast::TypeExpr,
    bindings: &BTreeMap<String, crate::ast::TypeExpr>,
    queue: &mut Vec<(String, Vec<crate::ast::TypeExpr>)>,
) -> crate::ast::TypeExpr {
    match ty {
        crate::ast::TypeExpr::Formal { name, span } => {
            bindings
                .get(name)
                .cloned()
                .unwrap_or(crate::ast::TypeExpr::Formal {
                    name: name.clone(),
                    span: *span,
                })
        }
        crate::ast::TypeExpr::List { element, key, span } => crate::ast::TypeExpr::List {
            element: Box::new(substitute_type(element, bindings, queue)),
            key: key.clone(),
            span: *span,
        },
        crate::ast::TypeExpr::WhenQualified { value, base, span } => {
            crate::ast::TypeExpr::WhenQualified {
                value: value.clone(),
                base: Box::new(substitute_type(base, bindings, queue)),
                span: *span,
            }
        }
        crate::ast::TypeExpr::TemplateRef { name, args, span } => {
            let substituted: Vec<crate::ast::TypeExpr> = args
                .iter()
                .map(|arg| substitute_type(arg, bindings, queue))
                .collect();
            queue.push((name.clone(), substituted.clone()));
            crate::ast::TypeExpr::TemplateRef {
                name: name.clone(),
                args: substituted,
                span: *span,
            }
        }
        other => other.clone(),
    }
}

/// Substitute one method's signatures and body (8.3: the substitution
/// covers the whole template body, bodies included): parameter types,
/// the return type, and every type position inside the actions. Each
/// template reference the substitution uncovers joins the worklist.
fn substitute_method(
    mut method: MethodDecl,
    bindings: &BTreeMap<String, crate::ast::TypeExpr>,
    queue: &mut Vec<(String, Vec<crate::ast::TypeExpr>)>,
) -> MethodDecl {
    for param in &mut method.params {
        if let Some(ty) = &param.ty {
            param.ty = Some(substitute_type(ty, bindings, queue));
        }
    }
    if let Some(return_type) = &method.return_type {
        method.return_type = Some(substitute_type(return_type, bindings, queue));
    }
    substitute_statements(&mut method.body, bindings, queue);
    method
}

/// Substitute every type position inside a statement list, in place.
fn substitute_statements(
    statements: &mut [Stmt],
    bindings: &BTreeMap<String, crate::ast::TypeExpr>,
    queue: &mut Vec<(String, Vec<crate::ast::TypeExpr>)>,
) {
    for statement in statements {
        substitute_stmt(statement, bindings, queue);
    }
}

/// The type positions of one statement (8.3): local-variable types,
/// `new <type>` creations, casts, `is a` checks — recursively through
/// the statement tree.
fn substitute_stmt(
    statement: &mut Stmt,
    bindings: &BTreeMap<String, crate::ast::TypeExpr>,
    queue: &mut Vec<(String, Vec<crate::ast::TypeExpr>)>,
) {
    match statement {
        Stmt::VarDecl { ty, init, .. } => {
            if let Some(ty) = ty {
                *ty = substitute_type(ty, bindings, queue);
            }
            if let Some(init) = init {
                substitute_expr(init, bindings, queue);
            }
        }
        Stmt::Assign { target, value, .. } => {
            substitute_expr(target, bindings, queue);
            substitute_expr(value, bindings, queue);
        }
        Stmt::ExprStmt { expr, .. } => substitute_expr(expr, bindings, queue),
        Stmt::Return {
            value: Some(value), ..
        } => substitute_expr(value, bindings, queue),
        Stmt::If {
            cond,
            then_body,
            else_body,
            ..
        } => {
            substitute_expr(cond, bindings, queue);
            substitute_statements(then_body, bindings, queue);
            if let Some(else_body) = else_body {
                substitute_statements(else_body, bindings, queue);
            }
        }
        Stmt::While { cond, body, .. } => {
            substitute_expr(cond, bindings, queue);
            substitute_statements(body, bindings, queue);
        }
        Stmt::RepeatUntil { body, cond, .. } => {
            substitute_statements(body, bindings, queue);
            substitute_expr(cond, bindings, queue);
        }
        Stmt::Print { exprs, .. } => {
            for expr in exprs {
                substitute_expr(expr, bindings, queue);
            }
        }
        Stmt::Gen { expr, .. } => substitute_expr(expr, bindings, queue),
        Stmt::Check {
            condition,
            else_body,
            ..
        } => {
            substitute_expr(condition, bindings, queue);
            substitute_statements(else_body, bindings, queue);
        }
        Stmt::ForEach {
            using, list, body, ..
        } => {
            if let Some(using) = using {
                substitute_expr(using, bindings, queue);
            }
            substitute_expr(list, bindings, queue);
            substitute_statements(body, bindings, queue);
        }
        Stmt::ForTo { from, to, body, .. } => {
            substitute_expr(from, bindings, queue);
            substitute_expr(to, bindings, queue);
            substitute_statements(body, bindings, queue);
        }
        Stmt::Wait { cycles, .. } => substitute_expr(cycles, bindings, queue),
        Stmt::Start { call, .. } | Stmt::Compute { call, .. } => {
            substitute_expr(call, bindings, queue)
        }
        Stmt::Case {
            scrutinee, arms, ..
        } => {
            substitute_expr(scrutinee, bindings, queue);
            for (alternatives, body) in arms {
                for expr in alternatives {
                    substitute_expr(expr, bindings, queue);
                }
                substitute_statements(body, bindings, queue);
            }
        }
        Stmt::Emit { target, .. } => substitute_expr(target, bindings, queue),
        // Statements without expression or type positions (`break`,
        // `continue`, `stop_run`, sequence `do`) and the temporal
        // forms (a `TemporalExpr` carries no type positions) need no
        // substitution.
        _ => {}
    }
}

/// The type positions of one expression (8.3): `new <type>`,
/// `expr.as_a(<type>)`, `expr is [not] a <type>`,
/// `set_of_values(<type>)` — recursively through the expression tree.
fn substitute_expr(
    expr: &mut Expr,
    bindings: &BTreeMap<String, crate::ast::TypeExpr>,
    queue: &mut Vec<(String, Vec<crate::ast::TypeExpr>)>,
) {
    match expr {
        Expr::New { ty, inits, .. } => {
            if let Some(ty) = ty {
                *ty = substitute_type(ty, bindings, queue);
            }
            substitute_statements(inits, bindings, queue);
        }
        Expr::CastAs {
            expr: inner, ty, ..
        }
        | Expr::IsA {
            expr: inner, ty, ..
        } => {
            substitute_expr(inner, bindings, queue);
            *ty = substitute_type(ty, bindings, queue);
        }
        Expr::SetOfValues { ty, .. } => {
            *ty = substitute_type(ty, bindings, queue);
        }
        Expr::MemberAccess { object, .. } => substitute_expr(object, bindings, queue),
        Expr::Call { callee, args, .. } => {
            substitute_expr(callee, bindings, queue);
            for arg in args {
                substitute_expr(arg, bindings, queue);
            }
        }
        Expr::Index { base, index, .. } => {
            substitute_expr(base, bindings, queue);
            substitute_expr(index, bindings, queue);
        }
        Expr::Binary { lhs, rhs, .. } => {
            substitute_expr(lhs, bindings, queue);
            substitute_expr(rhs, bindings, queue);
        }
        Expr::Unary { expr: inner, .. } => substitute_expr(inner, bindings, queue),
        Expr::Paren { expr: inner, .. } => substitute_expr(inner, bindings, queue),
        Expr::Ternary {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            substitute_expr(cond, bindings, queue);
            substitute_expr(then_expr, bindings, queue);
            substitute_expr(else_expr, bindings, queue);
        }
        Expr::BitSlice {
            base, high, low, ..
        } => {
            substitute_expr(base, bindings, queue);
            substitute_expr(high, bindings, queue);
            substitute_expr(low, bindings, queue);
        }
        Expr::InSet { element, set, .. } => {
            substitute_expr(element, bindings, queue);
            match set {
                crate::ast::InSetExpr::Items(items) => {
                    for item in items {
                        substitute_expr(item, bindings, queue);
                    }
                }
                crate::ast::InSetExpr::Range(low, high) => {
                    substitute_expr(low, bindings, queue);
                    substitute_expr(high, bindings, queue);
                }
            }
        }
        // Literals, identifiers, HDL paths and select distributions
        // (choices are raw tokens) carry no type positions.
        _ => {}
    }
}

/// 8.2's application rule with 8.3's NOTE 3: an extension applies to an
/// instance only if every actual complies with the specification its
/// position's parameter names — the `type` category (and a bare tag)
/// imposes no constraint, the `scalar` and `enumerated` categories
/// check the actual's shape, and any other spelling reads as 8.2's
/// specific-type form: the actual must render to exactly that name.
///
/// The two categories implemented are the ones the extractable prose
/// names (8.3's scalar example, 8.2.1's enumerated-type reading); the
/// full category table of 8.1.2 is an image in the local redline copy,
/// so any other category spelling cannot be honored — spelled as a
/// specific type there, it can only ever mismatch (never a false
/// application). The original template's own parameter categories stay
/// unvalidated (8.3's NOTE 2), consistent with their treatment before
/// extensions landed.
fn extension_applies(
    extension: &Declaration,
    actuals: &[crate::ast::TypeExpr],
    types: &[ElaboratedType],
) -> bool {
    for (param, actual) in extension.template_params.iter().zip(actuals) {
        let Some((_, category)) = param.name.split_once('\'') else {
            continue;
        };
        let complies = match category {
            "type" => true,
            "scalar" => actual_is_scalar(actual, types),
            "enumerated" => actual_is_enumerated(actual, types),
            specific => actual.render() == specific,
        };
        if !complies {
            return false;
        }
    }
    true
}

/// Category compliance for `scalar`: a predefined scalar base, an
/// inline enumeral list, or a declared scalar type — 5.1's prose reads
/// scalar types as the numeric, Boolean and enumerated types (so an
/// enumerated type complies; a struct does not, 8.3's map-of-structs
/// example).
fn actual_is_scalar(actual: &crate::ast::TypeExpr, types: &[ElaboratedType]) -> bool {
    match actual {
        crate::ast::TypeExpr::Scalar { .. } | crate::ast::TypeExpr::InlineEnum { .. } => true,
        crate::ast::TypeExpr::Named { name, .. } => types.iter().any(|ty| {
            &ty.name == name
                && ty.kind == DeclarationKind::Type
                && ty.underlying.as_ref().is_none_or(|underlying| {
                    matches!(underlying, crate::ast::TypeExpr::Scalar { .. })
                })
        }),
        _ => false,
    }
}

/// Category compliance for `enumerated`: an inline enumeral list or a
/// declared `type` with enumerals — 8.2.1's reading that the extension
/// applies only to the instances whose parameter is an enumerated type.
fn actual_is_enumerated(actual: &crate::ast::TypeExpr, types: &[ElaboratedType]) -> bool {
    match actual {
        crate::ast::TypeExpr::InlineEnum { .. } => true,
        crate::ast::TypeExpr::Named { name, .. } => types.iter().any(|ty| {
            &ty.name == name && ty.kind == DeclarationKind::Type && !ty.enumerals.is_empty()
        }),
        _ => false,
    }
}

/// Seed the monomorphization worklist from one type's member types and
/// method bodies (8.3: a body's `new <template> of <actual>` is a
/// reference to that instance).
fn enqueue_member_refs(
    entry: &ElaboratedType,
    queue: &mut Vec<(String, Vec<crate::ast::TypeExpr>)>,
) {
    for field in &entry.fields {
        collect_template_refs(&field.ty, queue);
    }
    for method in &entry.methods {
        for param in &method.params {
            if let Some(ty) = &param.ty {
                collect_template_refs(ty, queue);
            }
        }
        if let Some(return_type) = &method.return_type {
            collect_template_refs(return_type, queue);
        }
        for statement in &method.body {
            collect_statement_refs(statement, queue);
        }
    }
}

fn collect_template_refs(
    ty: &crate::ast::TypeExpr,
    queue: &mut Vec<(String, Vec<crate::ast::TypeExpr>)>,
) {
    match ty {
        crate::ast::TypeExpr::TemplateRef { name, args, .. } => {
            queue.push((name.clone(), args.clone()));
            for arg in args {
                collect_template_refs(arg, queue);
            }
        }
        crate::ast::TypeExpr::List { element, .. } => collect_template_refs(element, queue),
        crate::ast::TypeExpr::WhenQualified { base, .. } => collect_template_refs(base, queue),
        _ => {}
    }
}

/// The worklist seeds of the template references in one statement —
/// the read-only form of [`substitute_stmt`].
fn collect_statement_refs(
    statement: &crate::ast::Stmt,
    queue: &mut Vec<(String, Vec<crate::ast::TypeExpr>)>,
) {
    match statement {
        crate::ast::Stmt::VarDecl { ty, init, .. } => {
            if let Some(ty) = ty {
                collect_template_refs(ty, queue);
            }
            if let Some(init) = init {
                collect_expression_refs(init, queue);
            }
        }
        crate::ast::Stmt::Assign { target, value, .. } => {
            collect_expression_refs(target, queue);
            collect_expression_refs(value, queue);
        }
        crate::ast::Stmt::ExprStmt { expr, .. } => collect_expression_refs(expr, queue),
        crate::ast::Stmt::Return {
            value: Some(value), ..
        } => collect_expression_refs(value, queue),
        crate::ast::Stmt::If {
            cond,
            then_body,
            else_body,
            ..
        } => {
            collect_expression_refs(cond, queue);
            for statement in then_body {
                collect_statement_refs(statement, queue);
            }
            if let Some(else_body) = else_body {
                for statement in else_body {
                    collect_statement_refs(statement, queue);
                }
            }
        }
        crate::ast::Stmt::While { cond, body, .. } => {
            collect_expression_refs(cond, queue);
            for statement in body {
                collect_statement_refs(statement, queue);
            }
        }
        crate::ast::Stmt::RepeatUntil { body, cond, .. } => {
            for statement in body {
                collect_statement_refs(statement, queue);
            }
            collect_expression_refs(cond, queue);
        }
        crate::ast::Stmt::Print { exprs, .. } => {
            for expr in exprs {
                collect_expression_refs(expr, queue);
            }
        }
        crate::ast::Stmt::Gen { expr, .. } => collect_expression_refs(expr, queue),
        crate::ast::Stmt::Check {
            condition,
            else_body,
            ..
        } => {
            collect_expression_refs(condition, queue);
            for statement in else_body {
                collect_statement_refs(statement, queue);
            }
        }
        crate::ast::Stmt::ForEach {
            using, list, body, ..
        } => {
            if let Some(using) = using {
                collect_expression_refs(using, queue);
            }
            collect_expression_refs(list, queue);
            for statement in body {
                collect_statement_refs(statement, queue);
            }
        }
        crate::ast::Stmt::ForTo { from, to, body, .. } => {
            collect_expression_refs(from, queue);
            collect_expression_refs(to, queue);
            for statement in body {
                collect_statement_refs(statement, queue);
            }
        }
        crate::ast::Stmt::Wait { cycles, .. } => collect_expression_refs(cycles, queue),
        crate::ast::Stmt::Start { call, .. } | crate::ast::Stmt::Compute { call, .. } => {
            collect_expression_refs(call, queue)
        }
        crate::ast::Stmt::Case {
            scrutinee, arms, ..
        } => {
            collect_expression_refs(scrutinee, queue);
            for (alternatives, body) in arms {
                for expr in alternatives {
                    collect_expression_refs(expr, queue);
                }
                for statement in body {
                    collect_statement_refs(statement, queue);
                }
            }
        }
        crate::ast::Stmt::Emit { target, .. } => collect_expression_refs(target, queue),
        _ => {}
    }
}

/// The worklist seeds of the template references in one expression —
/// the read-only form of [`substitute_expr`].
fn collect_expression_refs(
    expr: &crate::ast::Expr,
    queue: &mut Vec<(String, Vec<crate::ast::TypeExpr>)>,
) {
    match expr {
        crate::ast::Expr::New { ty, inits, .. } => {
            if let Some(ty) = ty {
                collect_template_refs(ty, queue);
            }
            for statement in inits {
                collect_statement_refs(statement, queue);
            }
        }
        crate::ast::Expr::CastAs { expr, ty, .. } | crate::ast::Expr::IsA { expr, ty, .. } => {
            collect_expression_refs(expr, queue);
            collect_template_refs(ty, queue);
        }
        crate::ast::Expr::SetOfValues { ty, .. } => collect_template_refs(ty, queue),
        crate::ast::Expr::MemberAccess { object, .. } => collect_expression_refs(object, queue),
        crate::ast::Expr::Call { callee, args, .. } => {
            collect_expression_refs(callee, queue);
            for arg in args {
                collect_expression_refs(arg, queue);
            }
        }
        crate::ast::Expr::Index { base, index, .. } => {
            collect_expression_refs(base, queue);
            collect_expression_refs(index, queue);
        }
        crate::ast::Expr::Binary { lhs, rhs, .. } => {
            collect_expression_refs(lhs, queue);
            collect_expression_refs(rhs, queue);
        }
        crate::ast::Expr::Unary { expr, .. } => collect_expression_refs(expr, queue),
        crate::ast::Expr::Paren { expr, .. } => collect_expression_refs(expr, queue),
        crate::ast::Expr::Ternary {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_expression_refs(cond, queue);
            collect_expression_refs(then_expr, queue);
            collect_expression_refs(else_expr, queue);
        }
        crate::ast::Expr::BitSlice {
            base, high, low, ..
        } => {
            collect_expression_refs(base, queue);
            collect_expression_refs(high, queue);
            collect_expression_refs(low, queue);
        }
        crate::ast::Expr::InSet { element, set, .. } => {
            collect_expression_refs(element, queue);
            match set {
                crate::ast::InSetExpr::Items(items) => {
                    for item in items {
                        collect_expression_refs(item, queue);
                    }
                }
                crate::ast::InSetExpr::Range(low, high) => {
                    collect_expression_refs(low, queue);
                    collect_expression_refs(high, queue);
                }
            }
        }
        _ => {}
    }
}
