//! The coverage data model (clause 14): buckets, hits and grades for
//! the cover groups the parser records.
//!
//! Groups are built from the recorded [`CoverGroupDecl`]s at run start:
//! `is also` / `empty` extensions merge into their base group by name
//! (14.6), and each item's `using ranges` clause — recorded as raw
//! tokens in the shape `ranges = { range([lo..hi], "label"); ... }` —
//! is parsed into explicit buckets. Items without a `ranges` clause
//! get the clause 14.3 defaults: 16 buckets for integers ("buckets are
//! created by default for only the first 16 possible values (0 through
//! 15)" — the count is the `max_int_buckets` coverage configuration
//! option, 14.9) and the two values for booleans. Values falling
//! outside every bucket are ungradeable ("those values are given goals
//! of 0 and do not affect the grade for the item").
//!
//! The 14.9 coverage configuration ([`CoverageConfig`]) carries the
//! coverage mode (14.2's NOTE: "Unless coverage mode is turned on
//! first (see 14.9), no coverage results are collected") and
//! `max_int_buckets`. The setting routine the prose names,
//! `set_config(<category>, <option>, <value>)` (14.9), is not wired
//! into the interpreter yet, so the options are reachable through
//! [`build_with_config`] only. The option entries whose semantics the
//! extractable prose defines apply from the recorded tokens:
//! `no_collect`/`instance_no_collect` on the group (14.3.2) or the
//! item (14.4.4) — when TRUE the group/item "shall not be presented" —
//! the cross and transition `name` option (14.4/14.5), and the
//! group-level `at_least`/`weight` spellings (14.9.3/14.9.4: both "can
//! be set in the coverage group or item definitions").
//!
//! Not modeled yet: the `set_config()` routine itself, the bool-exp
//! option forms (including the implicit `inst` variable and the
//! `per_unit_instance` scoping of 14.3.2/14.4.4),
//! `instance_ignore`/`instance_illegal`, the `radix`/`when`/`never`
//! option semantics (Tables 40/41 are images in the local redline
//! copy), item `using also` extension overrides (14.7), and the
//! numeric parameters of `range(...)` beyond the bounds and label
//! (their semantics live in Table 41, an image in the local redline
//! copy — they are recorded and counted but not applied).

use crate::ast::{CoverGroupDecl, CoverItemDecl, CoverItemKind, Expr, ScalarBase, TypeExpr};
use crate::lexer::{Token, TokenKind};
use std::collections::BTreeMap;

/// One bucket of an item: an inclusive integer range with a hit goal
/// (1 unless a `range(...)` clause says otherwise) and the hits so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverBucket {
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
        if self.buckets.is_empty() {
            return None;
        }
        let covered = self
            .buckets
            .iter()
            .filter(|bucket| bucket.hits >= bucket.goal)
            .count();
        Some(covered as f64 / self.buckets.len() as f64)
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
        self.hits == 0
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
}

impl CoverGroupData {
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
            .filter(|item| item.per_instance && !item.buckets.is_empty())
            .collect();
        if per_instance.is_empty() {
            // The weighted average of the gradeable items' (basic and
            // derived) grades; weight 1 when the option is absent.
            let mut weighted_sum = 0.0f64;
            let mut weight_total = 0u64;
            for item in self.items.iter().filter(|item| !item.per_instance) {
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
            for bucket in &item.buckets {
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
            .filter(|item| !item.per_instance)
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
            if !item.per_instance {
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
        let item = self.items.iter().find(|item| item.name == item_name);
        let as_int = value_to_int(value);
        // The bucket label this value lands in (14.3's bucket model),
        // or the raw value when the item is unbucketed.
        let label = item.and_then(|item| item_label(item, as_int));
        let label = label.unwrap_or_else(|| render_value(value));
        // The transition over this item: previous → current (14.5).
        let previous = self.last_values.get(item_name).cloned();
        if let (Some(derived), Some(previous)) = (
            self.derived
                .iter_mut()
                .find(|derived| derived.sources == vec![item_name]),
            previous,
        ) {
            derived.record(&format!("{previous}→{label}"));
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

/// The coverage configuration options (14.9). The prose names the
/// setting mechanism — the `set_config()` predefined routine, "sets
/// the configuration options to the specified values" (the category,
/// option, value shape of its syntax example; 7.6.1's `set_config_max`
/// variant sets "the numeric options of a particular category to the
/// specified maximum values") — and two coverage options by name: the
/// coverage mode (14.2's NOTE) and `max_int_buckets` (14.3). The
/// routine itself is not wired into the interpreter yet, so the
/// options are reachable through [`build_with_config`] only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageConfig {
    /// 14.2's NOTE: "Unless coverage mode is turned on first (see
    /// 14.9), no coverage results are collected, even if cover groups
    /// and cover items are defined." With the mode off nothing is
    /// built, so the interpreter finds no groups to sample and the
    /// report presents none. The default keeps collection on — a
    /// documented deviation: the standard's mode starts off until
    /// `set_config` turns it on, and the routine is not wired yet.
    pub mode: bool,
    /// 14.3: "If ranges is not specified, the default number of
    /// buckets is 16 [set by the max_int_buckets coverage
    /// configuration option (see 14.9)]." The integer items' default
    /// bucket count; 0 leaves the item without default buckets
    /// (ungradeable).
    pub max_int_buckets: u64,
}

impl Default for CoverageConfig {
    fn default() -> Self {
        Self {
            mode: true,
            max_int_buckets: 16,
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
    // 14.2's NOTE (see [`CoverageConfig::mode`]).
    if !config.mode {
        return Vec::new();
    }
    let mut result: Vec<TypeCoverage> = Vec::new();
    for ty in &elaborated.types {
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
                name: decl.name.clone(),
                event_path: group_event_path(decl),
                items,
                derived,
                samples: 0,
                last_values: BTreeMap::new(),
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
        // 14.3.2: "if one or both of no_collect and instance_no_collect
        // options of the group are evaluated to TRUE, the group shall
        // not be presented under that instance." The OR relation of the
        // prose, over every declaration of the group; the constant TRUE
        // spelling drops the group from the collected model (not
        // sampled, not graded, not presented in the report). The
        // bool-exp form and the per-unit-instance scoping are not
        // modeled.
        groups.retain(|group| {
            !ty.covers.iter().any(|decl| {
                decl.name == group.name
                    && group.event_path == group_event_path(decl)
                    && decl.options.iter().any(|entry| {
                        scan_bool_option(entry, "no_collect") == Some(true)
                            || scan_bool_option(entry, "instance_no_collect") == Some(true)
                    })
            })
        });
        // Group-level `at_least`/`weight` (14.9.3/14.9.4: both "can be
        // set in the coverage group or item definitions"): the group
        // option applies to every item of the group that does not set
        // its own; a later declaration's reading overrides an earlier
        // one (14.6's override reading). The item-level spellings win
        // where present.
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
                    if let Some(Some(value)) = scan_uint_option(entry, "at_least") {
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
                        && scan_uint_option(&entry.using, "at_least").is_none()
                    {
                        for bucket in &mut item.buckets {
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
    // 14.4.4: "if one or both of no_collect and instance_no_collect
    // options of the item are evaluated to TRUE, the item shall not be
    // presented under that instance." The constant TRUE spelling (the
    // OR relation of the prose) drops the item from the collected
    // model; the bool-exp form and the per-unit-instance scoping are
    // not modeled.
    if scan_bool_option(&item.using, "no_collect") == Some(true)
        || scan_bool_option(&item.using, "instance_no_collect") == Some(true)
    {
        return None;
    }
    let name = item
        .names
        .first()
        .map(|(name, _)| name.clone())
        .unwrap_or_default();
    // The item options (Table 41 is an image in the local copy; the
    // common readings apply): `at_least = N` is the per-bucket goal,
    // `per_instance = TRUE` makes each bucket its own graded instance
    // (14.4.2), `weight = N` weights the grade.
    let at_least = scan_uint_option(&item.using, "at_least")
        .unwrap_or_default()
        .unwrap_or(1);
    let per_instance = scan_bool_option(&item.using, "per_instance").unwrap_or(false);
    let weight = scan_uint_option(&item.using, "weight")
        .unwrap_or_default()
        .unwrap_or(1);
    let mut ranges = Vec::new();
    if let Some(note) = parse_ranges(&item.using, &mut ranges) {
        return Some(CoverItemData {
            name,
            buckets: Vec::new(),
            ungradeable_hits: 0,
            samples: 0,
            note: Some(note),
            per_instance,
            weight,
        });
    }
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
    for bucket in &mut buckets {
        bucket.goal = at_least;
    }
    Some(CoverItemData {
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
    for index in 0..tokens.len() {
        if token_word(&tokens[index]) == Some(name) && token_kind_is(tokens.get(index + 1)?, "=") {
            let value = tokens.get(index + 2)?;
            return match &value.kind {
                TokenKind::Identifier(word) | TokenKind::Keyword(word) => {
                    Some(word == "TRUE" || word == "true")
                }
                _ => Some(false),
            };
        }
    }
    None
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
            label: "FALSE".to_owned(),
            lo: 0,
            hi: 0,
            goal: 1,
            hits: 0,
        },
        CoverBucket {
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

/// Parse `using ranges = { range([lo..hi], "label" [, num]*); ... }`
/// from the recorded `using` tokens. Returns `Some(reason)` when the
/// tokens carry a ranges clause this parser cannot read.
fn parse_ranges(using: &[Token], buckets: &mut Vec<CoverBucket>) -> Option<&'static str> {
    // Locate the `ranges` word followed by `= {`.
    let mut index = 0;
    while index < using.len() {
        if token_word(&using[index]) == Some("ranges")
            && using
                .get(index + 1)
                .is_some_and(|token| token_kind_is(token, "="))
        {
            break;
        }
        index += 1;
    }
    if index >= using.len() {
        return None; // no ranges clause at all
    }
    let mut cursor = index + 2;
    if !using
        .get(cursor)
        .is_some_and(|token| token_kind_is(token, "{"))
    {
        return Some("ranges clause is not a braced list");
    }
    cursor += 1;
    while cursor < using.len() {
        if token_kind_is(&using[cursor], "}") {
            return None;
        }
        if token_word(&using[cursor]) != Some("range") {
            return Some("unsupported entry in the ranges clause");
        }
        cursor += 1;
        let Some(parsed) = parse_range_entry(using, &mut cursor) else {
            return Some("unsupported range(...) shape");
        };
        buckets.push(parsed);
        // Consume the trailing ';'.
        if token_kind_is(using.get(cursor)?, ";") {
            cursor += 1;
        }
    }
    Some("ranges clause is not closed")
}

/// `([lo..hi], "label" [, num]*)` starting at `cursor['(']`; advances
/// the cursor past the entry.
fn parse_range_entry(tokens: &[Token], cursor: &mut usize) -> Option<CoverBucket> {
    if !token_kind_is(tokens.get(*cursor)?, "(") {
        return None;
    }
    *cursor += 1;
    if !token_kind_is(tokens.get(*cursor)?, "[") {
        return None;
    }
    *cursor += 1;
    // The bounds are constant integer expressions over the recorded
    // tokens (`0`, `0x21`, `MAX_UINT-9` — the corpus usage), evaluated
    // with the sampler's choice-expression evaluator.
    let mut lo_tokens = Vec::new();
    let ranged = loop {
        let token = tokens.get(*cursor)?;
        if token_kind_is(token, "..") {
            *cursor += 1;
            break true;
        }
        if token_kind_is(token, "]") {
            break false;
        }
        lo_tokens.push(token.clone());
        *cursor += 1;
    };
    let lo = crate::interp::eval_choice_int(&lo_tokens)?;
    let hi = if ranged {
        let mut hi_tokens = Vec::new();
        loop {
            let token = tokens.get(*cursor)?;
            if token_kind_is(token, "]") {
                break;
            }
            hi_tokens.push(token.clone());
            *cursor += 1;
        }
        crate::interp::eval_choice_int(&hi_tokens)?
    } else {
        // `range([v], ..)` — a single-value bucket has lo == hi.
        lo
    };
    // The bounds loops stop at the closing bracket without consuming it.
    if !token_kind_is(tokens.get(*cursor)?, "]") {
        return None;
    }
    *cursor += 1;
    let mut label = String::new();
    let mut goal = 1u64;
    // Optional `, "label"` and further parameters — the first positive
    // numeric value is the bucket's goal; `UNDEF` marks a parameter the
    // writer left unset (both per the corpus usage; the full option
    // semantics live in Table 41, an image in the local copy).
    while token_kind_is(tokens.get(*cursor)?, ",") {
        *cursor += 1;
        let token = tokens.get(*cursor)?;
        match &token.kind {
            TokenKind::StringLiteral(text) => {
                label = text.clone();
                *cursor += 1;
            }
            _ => {
                // A constant expression parameter (`10`, `MAX_UINT/20`,
                // `UNDEF`): evaluated with the choice-expression
                // evaluator, the first positive value taken as the
                // bucket's goal; `UNDEF` evaluates to nothing and is
                // skipped, as are unknown expressions (Table 41 is an
                // image in the local copy).
                let mut expr_tokens = Vec::new();
                while let Some(token) = tokens.get(*cursor) {
                    if token_kind_is(token, ",") || token_kind_is(token, ")") {
                        break;
                    }
                    expr_tokens.push(token.clone());
                    *cursor += 1;
                }
                if let Some(value) = crate::interp::eval_choice_int(&expr_tokens)
                    && goal == 1
                    && value > 0
                {
                    goal = value as u64;
                }
            }
        }
    }
    if !token_kind_is(tokens.get(*cursor)?, ")") {
        return None;
    }
    *cursor += 1;
    Some(CoverBucket {
        label,
        lo,
        hi,
        goal,
        hits: 0,
    })
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
