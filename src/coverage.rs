//! Coverage schemas, sampling data and grades (clause 14).
//!
//! Builders compose base and extended groups, default/explicit buckets,
//! derived items and supported option expressions while retaining source
//! spans. Runtime configuration (29.9) controls collection independently
//! of schema construction; the interpreter starts with coverage disabled.
//!
//! `per_instance` partitions grades by item bucket. `per_unit_instance`
//! instead collects separate groups for containing unit objects. Hidden
//! groups/items retain their sampling storage for derived items, then are
//! removed from the reported view. Group/basic-item `when` expressions
//! gate sampling in the owning struct context. Crosses require every
//! source in the current round; transitions retain the last sampled value.
//!
//! Structural option/scope errors report E0224 after composition. The
//! interpreter evaluates Boolean filters and per-unit visibility. Full
//! option type checking, instance_ignore/instance_illegal, general
//! ignore/illegal/radix semantics, real/enumeral ranges and general
//! constant/type resolution for range parameters,
//! item `using also` overrides and the wider runtime configuration API
//! remain unfinished. No conformance is claimed.

use crate::ast::{CoverGroupDecl, CoverItemDecl, CoverItemKind, Expr, ScalarBase, TypeExpr};
use crate::lexer::{Token, TokenKind};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ItemOptions {
    pub hidden: bool,
    pub when: Option<Expr>,
    pub instance_no_collect: Option<Expr>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct GroupOptions {
    pub hidden: bool,
    pub when: Option<Expr>,
    pub unit_scope: Option<String>,
    pub instance_no_collect: Option<Expr>,
}

/// One bucket of an item: an inclusive integer range with a hit goal
/// (1 unless a `range(...)` clause says otherwise) and the hits so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverBucket {
    /// Preserve range-local precedence over item/group/global defaults.
    pub(crate) explicit_goal: bool,
    pub label: String,
    pub lo: i128,
    pub hi: i128,
    pub goal: u64,
    pub hits: u64,
}

/// One item's collected data. `buckets` empty means the item is
/// ungradeable (its values are counted but contribute no grade).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverItemData {
    pub(crate) options: ItemOptions,
    pub name: String,
    pub buckets: Vec<CoverBucket>,
    /// Samples whose value fell outside every bucket (or had no bucket
    /// set at all).
    pub ungradeable_hits: u64,
    pub samples: u64,
    /// Why the item has no buckets, when it does not.
    pub note: Option<&'static str>,
    /// 14.4.2: the `per_instance = TRUE` option — each bucket becomes
    /// its own graded instance.
    pub per_instance: bool,
    /// The `weight` option (Table 41 is an image in the local copy;
    /// the common reading — a grade multiplier — is applied).
    pub weight: u64,
}

impl CoverItemData {
    /// The item's grade (14.3): the fraction of buckets that reached
    /// their goal; `None` when the item is ungradeable.
    pub fn grade(&self) -> Option<f64> {
        let total = self.graded_bucket_count();
        if total == 0 {
            return None;
        }
        let covered = self
            .buckets
            .iter()
            .filter(|bucket| bucket.goal > 0 && bucket.hits >= bucket.goal)
            .count();
        Some(covered as f64 / total as f64)
    }

    pub fn graded_bucket_count(&self) -> usize {
        self.buckets.iter().filter(|bucket| bucket.goal > 0).count()
    }
}

/// One per-instance unit of a `per_instance` item (14.4.2): "an
/// instance is created for every valid bucket of the per_instance
/// item. Any instance that is not sampled is marked as a hole." The
/// instance name is the per_type item's name, `==`, and the bucket's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverInstanceData {
    pub name: String,
    pub hits: u64,
    pub goal: u64,
    /// The owning item's weight (the group grade is the weighted
    /// grades of the per_instance items).
    pub weight: u64,
}

impl CoverInstanceData {
    /// The instance's grade: covered (1.0) when the hits reach the
    /// goal, 0.0 otherwise; unsampled instances are holes.
    pub fn grade(&self) -> f64 {
        if self.hits >= self.goal { 1.0 } else { 0.0 }
    }

    pub fn is_hole(&self) -> bool {
        self.goal > 0 && self.hits == 0
    }
}

/// A derived coverage item (14.4/14.5): a cross over the group's items
/// or a transition over one item's consecutive samples. Each sampling
/// event records one combination — the crossed items' bucket labels,
/// or the base item's previous→current labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverDerivedData {
    /// The 14.4/14.5 default names: `cross_a__b` and
    /// `transition__base`.
    pub name: String,
    /// The crossed item names, or the single base item for a
    /// transition.
    pub sources: Vec<String>,
    pub samples: u64,
    /// Each observed combination (rendered, e.g. `low→high` or
    /// `(low, high)`) with its hits.
    pub combos: Vec<(String, u64)>,
    /// The total combination count when gradeable: the cross product
    /// of the sources' bucket counts (transitions: n², every ordered
    /// pair of the base's buckets — Table 44 is an image in the local
    /// copy, so this v1 reading follows the bucket model). `None` when
    /// any source is unbucketed.
    pub total_combos: Option<u64>,
}

impl CoverDerivedData {
    /// The derived item's grade: the fraction of combinations observed
    /// at least once; `None` when not gradeable.
    pub fn grade(&self) -> Option<f64> {
        let total = self.total_combos?;
        if total == 0 {
            return None;
        }
        Some(self.combos.len() as f64 / total as f64)
    }

    fn record(&mut self, combo: &str) {
        self.samples += 1;
        if let Some(entry) = self.combos.iter_mut().find(|(seen, _)| seen == combo) {
            entry.1 += 1;
        } else {
            self.combos.push((combo.to_owned(), 1));
        }
    }
}

/// One coverage group's collected data (a type's base group merged with
/// its `is also` extensions).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverGroupData {
    pub(crate) options: GroupOptions,
    /// The containing unit's path for per-unit-instance collection.
    /// None identifies the type-level schema/collection.
    pub unit_instance: Option<String>,
    pub(crate) unit_owner: Option<usize>,
    pub name: String,
    /// The sampling event as a dotted path, when it resolves to one.
    pub event_path: Option<String>,
    pub items: Vec<CoverItemData>,
    /// Derived items (crosses and transitions, 14.4/14.5).
    pub derived: Vec<CoverDerivedData>,
    pub samples: u64,
    /// Each item's containing-bucket label (or raw value) at the last
    /// sampling event — the transition source (14.5's previous-value).
    last_values: BTreeMap<String, String>,
    round_items: BTreeSet<String>,
}

impl CoverGroupData {
    /// Start a sampling round before recording item values and crosses.
    /// Sources omitted in this round cannot contribute stale cross values.
    pub fn begin_sample(&mut self) {
        self.samples += 1;
        self.round_items.clear();
    }

    pub(crate) fn is_reported(&self) -> bool {
        !self.options.hidden && (self.options.unit_scope.is_none() || self.unit_owner.is_some())
    }
    /// The group's grade (14.3/14.4.2). With no `per_instance` items:
    /// the plain average of the gradeable items' (basic and derived)
    /// grades. With `per_instance` items: "the grade of the cover
    /// group is the weighted grades of all the per_instance items" —
    /// one weighted unit per instance (per valid bucket), plus the
    /// per_type data as one bucket (14.4.2's "sum of all valid buckets
    /// for all the per_instance items +1 (the per_type bucket)"), whose
    /// grade is the average of the per_type items' grades. `None` when
    /// nothing is gradeable.
    pub fn grade(&self) -> Option<f64> {
        let per_instance: Vec<&CoverItemData> = self
            .items
            .iter()
            .filter(|item| {
                !item.options.hidden && item.per_instance && item.graded_bucket_count() > 0
            })
            .collect();
        if per_instance.is_empty() {
            // The weighted average of the gradeable items' (basic and
            // derived) grades; weight 1 when the option is absent.
            let mut weighted_sum = 0.0f64;
            let mut weight_total = 0u64;
            for item in self
                .items
                .iter()
                .filter(|item| !item.options.hidden && !item.per_instance)
            {
                if let Some(grade) = item.grade() {
                    weighted_sum += grade * item.weight as f64;
                    weight_total += item.weight;
                }
            }
            for item in &self.derived {
                if let Some(grade) = item.grade() {
                    weighted_sum += grade;
                    weight_total += 1;
                }
            }
            if weight_total == 0 {
                return None;
            }
            return Some(weighted_sum / weight_total as f64);
        }
        let mut weighted_sum = 0.0f64;
        let mut weight_total = 0u64;
        for item in &per_instance {
            for bucket in item.buckets.iter().filter(|bucket| bucket.goal > 0) {
                let instance = CoverInstanceData {
                    name: format!("{}=={}", item.name, bucket.label),
                    hits: bucket.hits,
                    goal: bucket.goal,
                    weight: item.weight,
                };
                weighted_sum += instance.grade() * instance.weight as f64;
                weight_total += instance.weight;
            }
        }
        // The per_type bucket: one unit whose grade is the average of
        // the per_type (non-per-instance) gradeable items.
        let per_type_grades: Vec<f64> = self
            .items
            .iter()
            .filter(|item| !item.options.hidden && !item.per_instance)
            .filter_map(|item| item.grade())
            .collect();
        if !per_type_grades.is_empty() {
            let per_type_grade = per_type_grades.iter().sum::<f64>() / per_type_grades.len() as f64;
            weighted_sum += per_type_grade;
            weight_total += 1;
        }
        if weight_total == 0 {
            return None;
        }
        Some(weighted_sum / weight_total as f64)
    }

    /// The per-instance units (14.4.2) of the group's `per_instance`
    /// items, in item-then-bucket order.
    pub fn instances(&self) -> Vec<CoverInstanceData> {
        let mut instances = Vec::new();
        for item in &self.items {
            if item.options.hidden || !item.per_instance {
                continue;
            }
            for bucket in &item.buckets {
                instances.push(CoverInstanceData {
                    name: format!("{}=={}", item.name, bucket.label),
                    hits: bucket.hits,
                    goal: bucket.goal,
                    weight: item.weight,
                });
            }
        }
        instances
    }

    /// Record one sampled value for the named item, then update the
    /// transition/cross derived items (14.5: the transition derives
    /// `previous-value, value` pairs from the base item's consecutive
    /// samples; 14.4: the cross records every combination of the
    /// crossed items' values). The group's sample count is kept by the
    /// caller — once per sampling event, not once per item.
    pub fn sample(&mut self, item_name: &str, value: &crate::interp::Value) {
        self.round_items.insert(item_name.to_owned());
        let item = self.items.iter().find(|item| item.name == item_name);
        let as_int = value_to_int(value);
        // The bucket label this value lands in (14.3's bucket model),
        // or the raw value when the item is unbucketed.
        let label = item.and_then(|item| item_label(item, as_int));
        let label = label.unwrap_or_else(|| render_value(value));
        // The transition over this item: previous → current (14.5).
        let previous = self.last_values.get(item_name).cloned();
        if let Some(previous) = previous {
            for derived in self
                .derived
                .iter_mut()
                .filter(|derived| derived.sources == vec![item_name])
            {
                derived.record(&format!("{previous}→{label}"));
            }
        }
        self.last_values.insert(item_name.to_owned(), label.clone());
        // The crosses over combinations including this item record
        // after all items have their fresh labels, from record_round's
        // final pass — nothing to do here.
        let Some(item) = self.items.iter_mut().find(|item| item.name == item_name) else {
            return;
        };
        item.samples += 1;
        let Some(as_int) = as_int else {
            // Enumerands, strings, reals and lists are recorded but not
            // graded in this milestone.
            item.ungradeable_hits += 1;
            return;
        };
        if item.buckets.is_empty() {
            item.ungradeable_hits += 1;
            return;
        }
        if let Some(bucket) = item
            .buckets
            .iter_mut()
            .find(|bucket| as_int >= bucket.lo && as_int <= bucket.hi)
        {
            bucket.hits += 1;
        } else {
            // Outside every bucket: ungradeable (14.3).
            item.ungradeable_hits += 1;
        }
    }

    /// The cross pass of one sampling event (14.4): record each cross
    /// item's combination from its sources' current labels.
    pub fn record_crosses(&mut self) {
        let crosses: Vec<(String, Vec<String>)> = self
            .derived
            .iter()
            .filter(|derived| derived.sources.len() > 1)
            .map(|derived| (derived.name.clone(), derived.sources.clone()))
            .collect();
        for (name, sources) in crosses {
            if !sources
                .iter()
                .all(|source| self.round_items.contains(source))
            {
                continue;
            }
            let labels: Option<Vec<String>> = sources
                .iter()
                .map(|source| self.last_values.get(source).cloned())
                .collect();
            let Some(labels) = labels else { continue };
            let combo = format!("({})", labels.join(", "));
            if let Some(derived) = self.derived.iter_mut().find(|derived| derived.name == name) {
                derived.record(&combo);
            }
        }
    }
}

fn value_to_int(value: &crate::interp::Value) -> Option<i128> {
    match value {
        crate::interp::Value::Int(value) => Some(*value),
        crate::interp::Value::Bool(true) => Some(1),
        crate::interp::Value::Bool(false) => Some(0),
        _ => None,
    }
}

/// The label of the bucket containing the value (14.3), or `None` when
/// the item has no buckets or the value misses them all.
fn item_label(item: &CoverItemData, as_int: Option<i128>) -> Option<String> {
    let as_int = as_int?;
    let bucket = item
        .buckets
        .iter()
        .find(|bucket| as_int >= bucket.lo && as_int <= bucket.hi)?;
    if bucket.label.is_empty() {
        Some(format!("{as_int}"))
    } else {
        Some(bucket.label.clone())
    }
}

fn render_value(value: &crate::interp::Value) -> String {
    match value {
        crate::interp::Value::Int(value) => format!("{value}"),
        crate::interp::Value::Bool(value) => format!("{value}"),
        crate::interp::Value::Enum { name, .. } => name.clone(),
        _ => "?".to_owned(),
    }
}

/// One type's coverage groups (the type's own groups after merging its
/// `is also`/`empty` extensions).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TypeCoverage {
    pub type_name: String,
    pub groups: Vec<CoverGroupData>,
}

impl TypeCoverage {
    /// The groups of the type that sample on the emitted event path.
    pub fn groups_sampling(&self, event_path: &str) -> Vec<&CoverGroupData> {
        self.groups
            .iter()
            .filter(|group| group.event_path.as_deref() == Some(event_path))
            .collect()
    }
}

/// Coverage schema options: enablement (14.2) and default integer
/// bucket count (14.3). The interpreter exposes the corresponding
/// runtime options through `set_config(cover, ...)` (29.9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageConfig {
    /// With this option disabled, the builder produces no groups.
    /// The builder default constructs an enabled schema;
    /// runtime execution separately starts with collection disabled
    /// until `set_config(cover, mode, normal)` enables it.
    pub mode: bool,
    /// Default integer bucket count when no ranges are specified
    /// (14.3); 0 leaves the item without default buckets
    /// (ungradeable).
    pub max_int_buckets: u64,
    /// Default hit goal, overridden by group, item and range settings.
    pub at_least: u64,
}

impl Default for CoverageConfig {
    fn default() -> Self {
        Self {
            mode: true,
            max_int_buckets: 16,
            at_least: 1,
        }
    }
}

/// Build the coverage data model from the elaborated types' recorded
/// cover groups: one [`TypeCoverage`] per type that declares (or
/// extends) groups, extensions merged into their base by name (14.6),
/// in any declaration order. Uses the default [`CoverageConfig`].
pub fn build(elaborated: &crate::elab::Elaborated) -> Vec<TypeCoverage> {
    build_with_config(elaborated, &CoverageConfig::default())
}

/// [`build`] with explicit 14.9 coverage configuration options.
pub fn build_with_config(
    elaborated: &crate::elab::Elaborated,
    config: &CoverageConfig,
) -> Vec<TypeCoverage> {
    build_types(&elaborated.types, config)
}

pub(crate) fn build_types(
    types: &[crate::elab::ElaboratedType],
    config: &CoverageConfig,
) -> Vec<TypeCoverage> {
    // 14.2's NOTE (see [`CoverageConfig::mode`]).
    if !config.mode {
        return Vec::new();
    }
    let mut result: Vec<TypeCoverage> = Vec::new();
    for ty in types {
        if ty.covers.is_empty() {
            continue;
        }
        let mut groups: Vec<CoverGroupData> = Vec::new();
        // Pass 1: base groups and empty placeholders, in source order.
        for decl in &ty.covers {
            if decl.is_also {
                continue;
            }
            let items: Vec<CoverItemData> = decl
                .items
                .iter()
                .filter_map(|item| build_item(item, config))
                .collect();
            let derived = build_derived(decl, &items);
            groups.push(CoverGroupData {
                options: group_options(ty, &decl.name).unwrap_or_default(),
                unit_instance: None,
                unit_owner: None,
                name: decl.name.clone(),
                event_path: group_event_path(decl),
                items,
                derived,
                samples: 0,
                last_values: BTreeMap::new(),
                round_items: BTreeSet::new(),
            });
        }
        // Pass 2: merge every extension's items into its base by name
        // (an `is also` declared before its base still finds it here).
        for decl in &ty.covers {
            if !decl.is_also && !decl.empty {
                continue;
            }
            if let Some(base) = groups
                .iter_mut()
                .find(|group| group.name == decl.name && group.event_path == group_event_path(decl))
            {
                base.items.extend(
                    decl.items
                        .iter()
                        .filter_map(|item| build_item(item, config)),
                );
                base.derived.extend(build_derived(decl, &base.items));
            }
        }
        // Group-level options: `per_instance = TRUE` marks every item
        // per-instance (the group-level spelling of 14.4.2).
        for decl in &ty.covers {
            if decl.is_also {
                continue;
            }
            let group_per_instance = decl
                .options
                .iter()
                .any(|entry| scan_bool_option(entry, "per_instance") == Some(true));
            if group_per_instance
                && let Some(group) = groups.iter_mut().find(|group| {
                    group.name == decl.name && group.event_path == group_event_path(decl)
                })
            {
                for item in &mut group.items {
                    item.per_instance = true;
                }
            }
        }
        // Group defaults (14.9.3/14.9.4) apply where an item has no
        // override; explicit range goals retain the highest precedence.
        // Later group declarations replace earlier group defaults.
        for group in groups.iter_mut() {
            let matching: Vec<&CoverGroupDecl> = ty
                .covers
                .iter()
                .filter(|decl| {
                    decl.name == group.name && group.event_path == group_event_path(decl)
                })
                .collect();
            let mut group_at_least: Option<u64> = None;
            let mut group_weight: Option<u64> = None;
            for decl in &matching {
                for entry in &decl.options {
                    if let Ok(Some(value)) = coverage_goal(entry) {
                        group_at_least = Some(value);
                    }
                    if let Some(Some(value)) = scan_uint_option(entry, "weight") {
                        group_weight = Some(value);
                    }
                }
            }
            if group_at_least.is_none() && group_weight.is_none() {
                continue;
            }
            for decl in &matching {
                for entry in &decl.items {
                    if entry.kind != CoverItemKind::Item {
                        continue;
                    }
                    let Some(name) = entry.names.first().map(|(name, _)| name.clone()) else {
                        continue;
                    };
                    let Some(item) = group.items.iter_mut().find(|item| item.name == name) else {
                        continue;
                    };
                    if let Some(goal) = group_at_least
                        && coverage_goal(&entry.using).ok().flatten().is_none()
                    {
                        for bucket in item
                            .buckets
                            .iter_mut()
                            .filter(|bucket| !bucket.explicit_goal)
                        {
                            bucket.goal = goal;
                        }
                    }
                    if let Some(weight) = group_weight
                        && scan_uint_option(&entry.using, "weight").is_none()
                    {
                        item.weight = weight;
                    }
                }
            }
        }
        result.push(TypeCoverage {
            type_name: ty.name.clone(),
            groups,
        });
    }
    result
}

/// Synthesize a group's derived items (14.4/14.5) from its recorded
/// cross/transition entries, with totals over the basic items' buckets.
fn build_derived(decl: &CoverGroupDecl, items: &[CoverItemData]) -> Vec<CoverDerivedData> {
    let mut derived = Vec::new();
    for entry in &decl.items {
        match entry.kind {
            CoverItemKind::Transition => {
                let Some((base, _)) = entry.names.first() else {
                    continue;
                };
                let total = items
                    .iter()
                    .find(|item| &item.name == base)
                    .filter(|item| !item.buckets.is_empty())
                    .map(|item| (item.buckets.len() * item.buckets.len()) as u64);
                // 14.5: "If no name is specified for the transition item
                // with the name option, it gets a default name of
                // transition item-name" (two underscores).
                let name = scan_string_option(&entry.using, "name")
                    .unwrap_or_else(|| format!("transition__{base}"));
                derived.push(CoverDerivedData {
                    name,
                    sources: vec![base.clone()],
                    samples: 0,
                    combos: Vec::new(),
                    total_combos: total,
                });
            }
            CoverItemKind::Cross => {
                let sources: Vec<String> =
                    entry.names.iter().map(|(name, _)| name.clone()).collect();
                if sources.is_empty() {
                    continue;
                }
                let bucket_counts: Option<Vec<&CoverItemData>> = sources
                    .iter()
                    .map(|source| items.iter().find(|item| item.name == *source))
                    .collect();
                let total = bucket_counts.and_then(|items| {
                    items.iter().try_fold(1u64, |product, item| {
                        if item.buckets.is_empty() {
                            None
                        } else {
                            Some(product * item.buckets.len() as u64)
                        }
                    })
                });
                // 14.4: "It creates a new item with a name specified
                // using a name option or by using the default name of
                // cross item-name-1 item-name-2" (two underscores).
                let name = scan_string_option(&entry.using, "name")
                    .unwrap_or_else(|| format!("cross_{}", sources.join("__")));
                derived.push(CoverDerivedData {
                    name,
                    sources,
                    samples: 0,
                    combos: Vec::new(),
                    total_combos: total,
                });
            }
            CoverItemKind::Item => {}
        }
    }
    derived
}

/// The group's sampling event as a dotted path (`cover_trans`,
/// `sys.done`), when the recorded expression is a plain path.
fn group_event_path(decl: &CoverGroupDecl) -> Option<String> {
    fn dotted(expr: &Expr, out: &mut Vec<String>) -> bool {
        match expr {
            Expr::Ident { name, .. } => {
                out.push(name.clone());
                true
            }
            Expr::MemberAccess { object, name, .. } => match dotted(object, out) {
                true => {
                    out.push(name.clone());
                    true
                }
                false => false,
            },
            _ => false,
        }
    }
    let mut segments = Vec::new();
    if dotted(&decl.event, &mut segments) {
        Some(segments.join("."))
    } else {
        None
    }
}

/// Build one item's data: explicit `ranges` buckets when the recorded
/// `using` tokens carry them, the 14.3 defaults otherwise.
fn build_item(item: &CoverItemDecl, config: &CoverageConfig) -> Option<CoverItemData> {
    if item.kind != CoverItemKind::Item {
        // cross/transition items are recorded, not collected yet.
        return None;
    }
    let name = item
        .names
        .first()
        .map(|(name, _)| name.clone())
        .unwrap_or_default();
    // Item defaults apply after range construction (14.3). Per-instance
    // grading and weights use the existing collection model (14.3.2).
    let at_least = coverage_goal(&item.using)
        .ok()
        .flatten()
        .unwrap_or(config.at_least);
    let per_instance = scan_bool_option(&item.using, "per_instance").unwrap_or(false);
    let weight = scan_uint_option(&item.using, "weight")
        .unwrap_or_default()
        .unwrap_or(1);
    let ranges = match parse_ranges(&item.using) {
        Ok(ranges) => ranges,
        Err(_) => {
            return Some(CoverItemData {
                options: item_options(item).unwrap_or_default(),
                name,
                buckets: Vec::new(),
                ungradeable_hits: 0,
                samples: 0,
                note: Some("invalid or unsupported coverage ranges"),
                per_instance,
                weight,
            });
        }
    };
    let (mut buckets, note) = if ranges.is_empty() {
        // 14.3 defaults: integers grade over the first `max_int_buckets`
        // values (16 unless the 14.9 configuration option says
        // otherwise) — a sized item's possible-value count caps the
        // bucket count at 2^width (a `uint(bits: 2)` has four possible
        // values, so four buckets); booleans over their two values;
        // anything else has no default bucket set and is ungradeable.
        match &item.ty {
            Some(ty) if type_is_bool(ty) => (bool_buckets(), None),
            Some(ty) if type_is_int(ty) => {
                let buckets = match declared_int_width(ty) {
                    Some(bits) => {
                        let possible = 1u128 << bits.min(100) as u32;
                        let count = possible.min(config.max_int_buckets as u128) as u64;
                        default_int_buckets(count)
                    }
                    None => default_int_buckets(config.max_int_buckets),
                };
                let note = if buckets.is_empty() {
                    Some("max_int_buckets is 0: no default buckets")
                } else {
                    None
                };
                (buckets, note)
            }
            _ => (
                Vec::new(),
                Some("no ranges clause: values are recorded, not graded"),
            ),
        }
    } else {
        (ranges, None)
    };
    for bucket in buckets.iter_mut().filter(|bucket| !bucket.explicit_goal) {
        bucket.goal = at_least;
    }
    Some(CoverItemData {
        options: item_options(item).unwrap_or_default(),
        name,
        buckets,
        ungradeable_hits: 0,
        samples: 0,
        note,
        per_instance,
        weight,
    })
}

/// Scan `name = <uint>` in recorded option tokens; `None` when the
/// option is absent, `Some(None)` when present but not a readable
/// uint.
fn scan_uint_option(tokens: &[Token], name: &str) -> Option<Option<u64>> {
    for index in 0..tokens.len() {
        if token_word(&tokens[index]) == Some(name) && token_kind_is(tokens.get(index + 1)?, "=") {
            let value = tokens.get(index + 2)?;
            return Some(match &value.kind {
                TokenKind::Number(text) => {
                    crate::interp::parse_int_literal(text).map(|v| v.max(0) as u64)
                }
                _ => None,
            });
        }
    }
    None
}

/// Scan `name = TRUE|FALSE` in recorded option tokens.
fn scan_bool_option(tokens: &[Token], name: &str) -> Option<bool> {
    let value = option_value(tokens, name)?;
    if value.is_empty() {
        return Some(true);
    }
    match value {
        [token] => match token_word(token)? {
            "TRUE" | "true" => Some(true),
            "FALSE" | "false" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// Scan `name = "text"` in recorded option tokens; `None` when the
/// option is absent or its value is not a string literal.
fn scan_string_option(tokens: &[Token], name: &str) -> Option<String> {
    for index in 0..tokens.len() {
        if token_word(&tokens[index]) == Some(name) && token_kind_is(tokens.get(index + 1)?, "=") {
            return match tokens.get(index + 2).map(|token| &token.kind) {
                Some(TokenKind::StringLiteral(text)) => Some(text.clone()),
                _ => None,
            };
        }
    }
    None
}

fn bool_buckets() -> Vec<CoverBucket> {
    vec![
        CoverBucket {
            explicit_goal: false,
            label: "FALSE".to_owned(),
            lo: 0,
            hi: 0,
            goal: 1,
            hits: 0,
        },
        CoverBucket {
            explicit_goal: false,
            label: "TRUE".to_owned(),
            lo: 1,
            hi: 1,
            goal: 1,
            hits: 0,
        },
    ]
}

/// The 14.3 default integer buckets: one bucket per value, "for only
/// the first 16 possible values (0 through 15)" — the count being the
/// `max_int_buckets` coverage configuration option's default (14.9).
fn default_int_buckets(max: u64) -> Vec<CoverBucket> {
    (0..max)
        .map(|value| CoverBucket {
            explicit_goal: false,
            label: format!("{value}"),
            lo: value as i128,
            hi: value as i128,
            goal: 1,
            hits: 0,
        })
        .collect()
}

fn type_is_bool(ty: &TypeExpr) -> bool {
    matches!(
        ty,
        TypeExpr::Scalar {
            base: ScalarBase::Bool,
            ..
        }
    )
}

/// The declared bit width of an integer scalar type, when it is a
/// literal (`uint(bits: 2)` → 2). The default bucket count of a sized
/// item caps at the type's possible-value count.
fn declared_int_width(ty: &TypeExpr) -> Option<i128> {
    let TypeExpr::Scalar {
        width: Some(width_expr),
        ..
    } = ty
    else {
        return None;
    };
    let Expr::IntLiteral { text, .. } = width_expr.as_ref() else {
        return None;
    };
    match crate::interp::parse_int_literal(text) {
        Some(bits) if bits > 0 => Some(bits),
        _ => None,
    }
}

fn type_is_int(ty: &TypeExpr) -> bool {
    matches!(
        ty,
        TypeExpr::Scalar {
            base: ScalarBase::Uint
                | ScalarBase::Int
                | ScalarBase::Longint
                | ScalarBase::Longuint
                | ScalarBase::Bit
                | ScalarBase::Byte
                | ScalarBase::Nibble,
            ..
        }
    )
}

/// Integral range construction (14.3). Width, name and hit goal are
/// positional parameters; unsupported forms are diagnosed, never guessed.
fn parse_ranges(using: &[Token]) -> Result<Vec<CoverBucket>, crate::diagnostic::Diagnostic> {
    let Some(tokens) = option_value(using, "ranges") else {
        return Ok(Vec::new());
    };
    if tokens.len() < 2
        || !token_kind_is(&tokens[0], "{")
        || !token_kind_is(tokens.last().unwrap(), "}")
    {
        return Err(option_error(
            tokens,
            "coverage ranges must be a braced list (14.3)",
        ));
    }
    let mut buckets = Vec::new();
    for entry in split_parameters(&tokens[1..tokens.len() - 1], ";") {
        if entry.is_empty() {
            continue;
        }
        if entry.len() < 4
            || token_word(&entry[0]) != Some("range")
            || !token_kind_is(&entry[1], "(")
            || !token_kind_is(entry.last().unwrap(), ")")
        {
            return Err(option_error(
                entry,
                "expected range(...) in coverage ranges (14.3)",
            ));
        }
        let parameters = split_parameters(&entry[2..entry.len() - 1], ",");
        if parameters.is_empty()
            || parameters.len() > 4
            || parameters.iter().any(|parameter| parameter.is_empty())
        {
            return Err(option_error(
                entry,
                "range expects one to four nonempty parameters (14.3)",
            ));
        }
        let bounds = parameters[0];
        if bounds.len() < 3
            || !token_kind_is(&bounds[0], "[")
            || !token_kind_is(bounds.last().unwrap(), "]")
        {
            return Err(option_error(
                bounds,
                "coverage range bounds must use brackets (14.3)",
            ));
        }
        let bounds = split_parameters(&bounds[1..bounds.len() - 1], "..");
        if bounds.len() > 2 || bounds.iter().any(|bound| bound.is_empty()) {
            return Err(option_error(
                parameters[0],
                "coverage ranges require one value or two finite bounds (14.3)",
            ));
        }
        let lo = constant_integer(bounds[0])?;
        let hi = if bounds.len() == 2 {
            constant_integer(bounds[1])?
        } else {
            lo
        };
        if lo > hi {
            return Err(option_error(
                parameters[0],
                "coverage range lower bound exceeds upper bound (14.3)",
            ));
        }
        let name = parameters
            .get(1)
            .map(|tokens| constant_string(tokens))
            .transpose()?
            .unwrap_or_default();
        let width = match parameters.get(2) {
            None => None,
            Some(tokens) if is_undef(tokens) => None,
            Some(tokens) => {
                let width = constant_integer(tokens)?;
                if width <= 0 {
                    return Err(option_error(
                        tokens,
                        "coverage range bucket width must be positive (14.3)",
                    ));
                }
                if !name.is_empty() {
                    return Err(option_error(
                        tokens,
                        "subdivided coverage ranges require an empty name (14.3)",
                    ));
                }
                Some(width)
            }
        };
        let goal = parameters
            .get(3)
            .filter(|tokens| !is_undef(tokens))
            .map(|tokens| unsigned_goal(tokens))
            .transpose()?;
        let count = match width {
            None => Some(1),
            Some(width) => hi
                .abs_diff(lo)
                .checked_div(width as u128)
                .and_then(|count| count.checked_add(1)),
        };
        let Some(count) =
            count.filter(|count| *count <= 65_536 && *count + buckets.len() as u128 <= 65_536)
        else {
            return Err(option_error(
                entry,
                "coverage ranges exceed the 65536 bucket construction limit (14.3)",
            ));
        };
        let mut start = lo;
        for _ in 0..count {
            let end = width.map_or(hi, |width| {
                start.checked_add(width - 1).unwrap_or(i128::MAX).min(hi)
            });
            let label = if !name.is_empty() {
                name.clone()
            } else if start == end {
                start.to_string()
            } else {
                format!("{start}..{end}")
            };
            buckets.push(CoverBucket {
                explicit_goal: goal.is_some(),
                label,
                lo: start,
                hi: end,
                goal: goal.unwrap_or(1),
                hits: 0,
            });
            if end == hi {
                break;
            }
            start = end + 1;
        }
    }
    if buckets.is_empty() {
        return Err(option_error(
            tokens,
            "coverage ranges require at least one range (14.3)",
        ));
    }
    Ok(buckets)
}

fn split_parameters<'a>(tokens: &'a [Token], separator: &str) -> Vec<&'a [Token]> {
    let mut result = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (index, token) in tokens.iter().enumerate() {
        if depth == 0 && token_kind_is(token, separator) {
            result.push(&tokens[start..index]);
            start = index + 1;
        } else if let TokenKind::Punctuation(symbol) = &token.kind {
            match symbol.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    if start < tokens.len() || separator != ";" {
        result.push(&tokens[start..]);
    }
    result
}

fn is_undef(tokens: &[Token]) -> bool {
    matches!(tokens, [token] if token_word(token) == Some("UNDEF"))
}

fn constant_integer(tokens: &[Token]) -> Result<i128, crate::diagnostic::Diagnostic> {
    fn evaluate(expr: &Expr) -> Option<i128> {
        use crate::ast::{BinaryOp, UnaryOp};
        match expr {
            Expr::IntLiteral { text, .. } | Expr::BitLiteral { text, .. } => {
                crate::interp::parse_int_literal(text)
            }
            Expr::Ident { name, .. } => match name.as_str() {
                "MAX_UINT" => Some(u32::MAX as i128),
                "MAX_INT" => Some(i32::MAX as i128),
                "MAX_LONGUINT" => Some(u64::MAX as i128),
                "MAX_LONGINT" => Some(i64::MAX as i128),
                "MIN_INT" => Some(i32::MIN as i128),
                "MIN_LONGINT" => Some(i64::MIN as i128),
                _ => None,
            },
            Expr::Paren { expr, .. } => evaluate(expr),
            Expr::Unary { op, expr, .. } => {
                let value = evaluate(expr)?;
                match op {
                    UnaryOp::Plus => Some(value),
                    UnaryOp::Negate => value.checked_neg(),
                    _ => None,
                }
            }
            Expr::Binary { op, lhs, rhs, .. } => {
                let (lhs, rhs) = (evaluate(lhs)?, evaluate(rhs)?);
                match op {
                    BinaryOp::Add => lhs.checked_add(rhs),
                    BinaryOp::Sub => lhs.checked_sub(rhs),
                    BinaryOp::Mul => lhs.checked_mul(rhs),
                    BinaryOp::Div => lhs.checked_div(rhs),
                    BinaryOp::Mod => lhs.checked_rem(rhs),
                    _ => None,
                }
            }
            _ => None,
        }
    }
    crate::parser::parse_expression_tokens(tokens).ok().and_then(|expr| evaluate(&expr))
        .ok_or_else(|| option_error(tokens, "coverage range parameters require supported constant integers without overflow (14.3)"))
}

fn constant_string(tokens: &[Token]) -> Result<String, crate::diagnostic::Diagnostic> {
    fn evaluate(expr: &Expr) -> Option<String> {
        match expr {
            Expr::StringLiteral { value, .. } => Some(value.clone()),
            Expr::Paren { expr, .. } => evaluate(expr),
            Expr::Binary {
                op: crate::ast::BinaryOp::Add,
                lhs,
                rhs,
                ..
            } => {
                let mut value = evaluate(lhs)?;
                value.push_str(&evaluate(rhs)?);
                Some(value)
            }
            _ => None,
        }
    }
    crate::parser::parse_expression_tokens(tokens)
        .ok()
        .and_then(|expr| evaluate(&expr))
        .ok_or_else(|| {
            option_error(
                tokens,
                "coverage range name requires a supported constant string (14.3)",
            )
        })
}

fn unsigned_goal(tokens: &[Token]) -> Result<u64, crate::diagnostic::Diagnostic> {
    u64::try_from(constant_integer(tokens)?).map_err(|_| {
        option_error(
            tokens,
            "coverage hit goals must be nonnegative u64 values (14.3)",
        )
    })
}

fn coverage_goal(tokens: &[Token]) -> Result<Option<u64>, crate::diagnostic::Diagnostic> {
    option_value(tokens, "at_least")
        .map(unsigned_goal)
        .transpose()
}

fn token_word(token: &Token) -> Option<&str> {
    match &token.kind {
        TokenKind::Identifier(word) | TokenKind::Keyword(word) => Some(word.as_str()),
        _ => None,
    }
}

fn token_kind_is(token: &Token, symbol: &str) -> bool {
    matches!(&token.kind, TokenKind::Punctuation(p) if p == symbol)
}

/// Split only top-level option separators; calls and range bodies keep
/// their internal commas/semicolons and their original source spans.
fn option_value<'a>(tokens: &'a [Token], name: &str) -> Option<&'a [Token]> {
    let mut start = 0;
    let mut depth = 0usize;
    let mut found = None;
    for end in 0..=tokens.len() {
        let separator = end == tokens.len()
            || (depth == 0
                && (token_kind_is(&tokens[end], ",") || token_kind_is(&tokens[end], ";")));
        if separator {
            let part = &tokens[start..end];
            if part.first().and_then(token_word) == Some(name) {
                found = Some(
                    if part.get(1).is_some_and(|token| token_kind_is(token, "=")) {
                        &part[2..]
                    } else {
                        &part[1..]
                    },
                );
            }
            start = end + 1;
        } else if let TokenKind::Punctuation(symbol) = &tokens[end].kind {
            match symbol.as_str() {
                "(" | "[" | "{" => depth += 1,
                ")" | "]" | "}" => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    found
}

fn option_error(tokens: &[Token], message: &str) -> crate::diagnostic::Diagnostic {
    let position = tokens
        .first()
        .map(|token| token.start)
        .unwrap_or(crate::lexer::Position {
            byte: 0,
            line: 1,
            column: 1,
        });
    crate::diagnostic::Diagnostic::new("E0224", message, position, position)
}

fn option_expr(
    tokens: &[Token],
    name: &str,
) -> Result<Option<Expr>, crate::diagnostic::Diagnostic> {
    let Some(value) = option_value(tokens, name) else {
        return Ok(None);
    };
    if value.is_empty() && name == "no_collect" {
        let token = tokens.first().expect("present option has a token");
        return Ok(Some(Expr::BoolLiteral {
            value: true,
            span: (token.start, token.end),
        }));
    }
    crate::parser::parse_expression_tokens(value)
        .map(Some)
        .map_err(|diagnostic| {
            let mut diagnostic = diagnostic;
            diagnostic.code = "E0224";
            diagnostic
        })
}

fn hidden_option(tokens: &[Token]) -> Result<bool, crate::diagnostic::Diagnostic> {
    match option_expr(tokens, "no_collect")? {
        None => Ok(false),
        Some(Expr::BoolLiteral { value, .. }) => Ok(value),
        Some(_) => Err(option_error(
            tokens,
            "no_collect expects a flag or Boolean literal (14.2)",
        )),
    }
}

fn item_options(item: &CoverItemDecl) -> Result<ItemOptions, crate::diagnostic::Diagnostic> {
    Ok(ItemOptions {
        hidden: hidden_option(&item.using)?,
        when: option_expr(&item.using, "when")?,
        instance_no_collect: option_expr(&item.using, "instance_no_collect")?,
    })
}

fn group_options(
    ty: &crate::elab::ElaboratedType,
    name: &str,
) -> Result<GroupOptions, crate::diagnostic::Diagnostic> {
    let mut options = GroupOptions::default();
    let mut scope_frozen = false;
    for decl in ty.covers.iter().filter(|decl| decl.name == name) {
        for entry in &decl.options {
            if let Some(value) = option_value(entry, "per_unit_instance") {
                let scope = if value.is_empty() {
                    if ty.kind != crate::ast::DeclarationKind::Unit {
                        return Err(option_error(
                            entry,
                            "per_unit_instance requires a unit type in a struct (14.2)",
                        ));
                    }
                    ty.name.clone()
                } else if value.len() == 1 {
                    token_word(&value[0])
                        .ok_or_else(|| {
                            option_error(entry, "per_unit_instance expects a unit type name (14.2)")
                        })?
                        .to_owned()
                } else {
                    return Err(option_error(
                        entry,
                        "per_unit_instance expects a unit type name (14.2)",
                    ));
                };
                if scope_frozen && options.unit_scope.as_ref() != Some(&scope) {
                    return Err(option_error(
                        entry,
                        "per_unit_instance cannot change after instance_no_collect (14.2.2)",
                    ));
                }
                options.unit_scope = Some(scope);
            }
            coverage_goal(entry)?;
            options.hidden |= hidden_option(entry)?;
            if let Some(expr) = option_expr(entry, "when")? {
                options.when = Some(expr);
            }
            if let Some(expr) = option_expr(entry, "instance_no_collect")? {
                options.instance_no_collect = Some(expr);
                scope_frozen = true;
            }
        }
        if decl
            .items
            .iter()
            .any(|item| option_value(&item.using, "instance_no_collect").is_some())
        {
            scope_frozen = true;
        }
    }
    Ok(options)
}

/// Structural coverage option checks run on the composed type view,
/// so inherited and extended groups use the same scope as execution.
pub(crate) fn validate_types(
    types: &[crate::elab::ElaboratedType],
) -> Vec<crate::diagnostic::Diagnostic> {
    let mut diagnostics = Vec::new();
    for ty in types {
        let mut seen = BTreeSet::new();
        for decl in &ty.covers {
            if !seen.insert(&decl.name) {
                continue;
            }
            let options = match group_options(ty, &decl.name) {
                Ok(options) => options,
                Err(diagnostic) => {
                    diagnostics.push(diagnostic);
                    continue;
                }
            };
            if let Some(scope) = &options.unit_scope {
                if !types
                    .iter()
                    .any(|ty| &ty.name == scope && ty.kind == crate::ast::DeclarationKind::Unit)
                {
                    diagnostics.push(crate::diagnostic::Diagnostic::new(
                        "E0224",
                        format!("coverage unit type '{scope}' is not a declared unit (14.2)"),
                        decl.name_span.0,
                        decl.name_span.1,
                    ));
                }
            } else if options.instance_no_collect.is_some() {
                diagnostics.push(crate::diagnostic::Diagnostic::new(
                    "E0224",
                    "instance_no_collect requires per_unit_instance (14.2.2)",
                    decl.name_span.0,
                    decl.name_span.1,
                ));
            }
            for item in ty
                .covers
                .iter()
                .filter(|group| group.name == decl.name)
                .flat_map(|group| &group.items)
            {
                if item.kind == CoverItemKind::Item {
                    if let Err(diagnostic) = coverage_goal(&item.using) {
                        diagnostics.push(diagnostic);
                    }
                    if let Some(tokens) = option_value(&item.using, "ranges") {
                        let unsupported = match &item.ty {
                            Some(TypeExpr::Scalar {
                                base: ScalarBase::String,
                                ..
                            }) => Some("coverage ranges are not permitted for string items (14.3)"),
                            Some(TypeExpr::Scalar {
                                base: ScalarBase::Real,
                                ..
                            }) => Some("real coverage ranges are not implemented (14.3)"),
                            _ => None,
                        };
                        if let Some(message) = unsupported {
                            diagnostics.push(option_error(tokens, message));
                        } else if let Err(diagnostic) = parse_ranges(&item.using) {
                            diagnostics.push(diagnostic);
                        }
                    }
                }
                match item_options(item) {
                    Err(diagnostic) => diagnostics.push(diagnostic),
                    Ok(item_options)
                        if item_options.instance_no_collect.is_some()
                            && options.unit_scope.is_none() =>
                    {
                        diagnostics.push(option_error(
                            &item.using,
                            "item instance_no_collect requires per_unit_instance (14.3.4)",
                        ));
                    }
                    _ => {}
                }
            }
        }
    }
    diagnostics
}

/// Hide presentation-only data after collection, preserving hidden
/// source items internally for derived sampling and grading.
pub(crate) fn for_report(coverage: &[TypeCoverage]) -> Vec<TypeCoverage> {
    let mut result = coverage.to_vec();
    for entry in &mut result {
        entry.groups.retain(CoverGroupData::is_reported);
        for group in &mut entry.groups {
            group.items.retain(|item| !item.options.hidden);
        }
    }
    result
}
