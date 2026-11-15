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
//! removed from the reported view. Group/basic/derived `when` expressions
//! gate sampling in the owning struct context. Crosses require every
//! source in the current round; transitions retain the last sampled value.
//! Crosses and transitions can consume previously declared derived items.
//! Source graphs use stable observation references, with a 65536 expanded
//! scalar-component bound. E0224 rejects invalid source order/names/shape.
//! Derived hit goals follow item > group > runtime defaults; weights use
//! item > group > 1.
//! Presentation-only hiding leaves underlying sampling data intact.
//!
//! Basic-item ignore/illegal predicates exclude observations and wholly
//! excluded buckets from goals. Instance variants bind the containing unit.
//! Integral/Boolean masks use interval classification or bounded exact
//! evaluation. Derived scalar predicates bind participating basic values and
//! transition prev_<source> snapshots; tuple masks use conservative interval
//! proofs, iterative splitting and bounded concrete checks. Composite operands remain unsupported. Derived histories
//! retain bucket identities across mask changes.
//!
//! Structural option/scope errors report E0224 after composition. The
//! interpreter evaluates Boolean filters and per-unit visibility. The
//! conservative type pass checks supported Boolean option expressions in
//! their owner/source/unit scopes. Full option type checking, predicates over composite-valued sources, radix
//! semantics, real/enumeral ranges and general
//! constant/type resolution for range parameters,
//! item `using also` overrides and the wider runtime configuration API
//! remain unfinished. No conformance is claimed.

use crate::ast::{CoverGroupDecl, CoverItemDecl, CoverItemKind, Expr, ScalarBase, TypeExpr};
use crate::lexer::{Token, TokenKind};
use std::collections::{BTreeMap, BTreeSet};

mod proof;
pub(crate) use proof::{ScalarDomain, prove_predicate};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ItemOptions {
    pub ignore: Option<Expr>,
    pub illegal: Option<Expr>,
    pub instance_ignore: Option<Expr>,
    pub instance_illegal: Option<Expr>,
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
    /// No legal, non-ignored values remain in this bucket.
    pub excluded: bool,
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
            .filter(|bucket| !bucket.excluded && bucket.goal > 0 && bucket.hits >= bucket.goal)
            .count();
        Some(covered as f64 / total as f64)
    }

    pub fn graded_bucket_count(&self) -> usize {
        self.buckets
            .iter()
            .filter(|bucket| !bucket.excluded && bucket.goal > 0)
            .count()
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum BucketIdentity {
    Basic(usize),
    Unbucketed,
    Outside,
    Derived(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ScalarSample {
    Int(i128),
    Bool(bool),
    Real(u64),
    Str(String),
    Enum(String, String),
}

impl ScalarSample {
    fn capture(value: &crate::interp::Value) -> Option<Self> {
        use crate::interp::Value;
        match value {
            Value::Int(value) => Some(Self::Int(*value)),
            Value::Bool(value) => Some(Self::Bool(*value)),
            Value::Real(value) => Some(Self::Real(value.to_bits())),
            Value::Str(value) => Some(Self::Str(value.clone())),
            Value::Enum { type_name, name } => Some(Self::Enum(type_name.clone(), name.clone())),
            _ => None,
        }
    }
    fn value(&self) -> crate::interp::Value {
        use crate::interp::Value;
        match self {
            Self::Int(value) => Value::Int(*value),
            Self::Bool(value) => Value::Bool(*value),
            Self::Real(value) => Value::Real(f64::from_bits(*value)),
            Self::Str(value) => Value::Str(value.clone()),
            Self::Enum(type_name, name) => Value::Enum {
                type_name: type_name.clone(),
                name: name.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Observation {
    scalar: Option<ScalarSample>,
    label: String,
    bucket: BucketIdentity,
}

struct SourceSummary {
    total: Option<u64>,
    basic: bool,
    valid: Vec<bool>,
}

impl SourceSummary {
    fn accepts(&self, bucket: &BucketIdentity) -> bool {
        match bucket {
            BucketIdentity::Basic(index) if self.basic => {
                self.valid.get(*index).copied().unwrap_or(false)
            }
            BucketIdentity::Unbucketed => self.basic && self.valid.is_empty(),
            BucketIdentity::Derived(index) if !self.basic => {
                self.valid.get(*index).copied().unwrap_or(false)
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DerivedKey {
    labels: Vec<String>,
    buckets: Vec<BucketIdentity>,
    hits: u64,
}

/// A derived coverage item (14.4/14.5): a cross over the group's items
/// or a transition over one item's consecutive samples. Each sampling
/// event records one combination — the crossed items' bucket labels,
/// or the base item's previous→current labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverDerivedData {
    pub(crate) kind: CoverItemKind,
    pub(crate) options: ItemOptions,
    pub(crate) collect: bool,
    /// None preserves the source domain. Some retains these basic-bucket
    /// tuples; an empty set excludes the entire node, including composites.
    pub(crate) allowed: Option<BTreeSet<Vec<usize>>>,
    explicit_goal: bool,
    explicit_weight: bool,
    /// Required observations per derived combination; zero is ungraded.
    pub goal: u64,
    /// Weight in the group's per-type grade.
    pub weight: u64,
    keys: Vec<DerivedKey>,
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
    /// reaching the hit goal; `None` when ungradeable or the goal is zero.
    pub fn grade(&self) -> Option<f64> {
        let total = self.total_combos?;
        if total == 0 || self.goal == 0 {
            return None;
        }
        let covered = self
            .combos
            .iter()
            .filter(|(_, hits)| *hits >= self.goal)
            .count();
        Some(covered as f64 / total as f64)
    }

    fn record(&mut self, labels: Vec<String>, buckets: Vec<BucketIdentity>) -> usize {
        let index = self
            .keys
            .iter()
            .position(|key| key.labels == labels && key.buckets == buckets);
        if let Some(index) = index {
            self.keys[index].hits += 1;
            index
        } else {
            self.keys.push(DerivedKey {
                labels,
                buckets,
                hits: 1,
            });
            self.keys.len() - 1
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
    last_values: BTreeMap<String, Observation>,
    prior_values: BTreeMap<String, Observation>,
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
    /// the weighted average of the gradeable items' (basic and derived)
    /// grades. With `per_instance` items: "the grade of the cover
    /// group is the weighted grades of all the per_instance items" —
    /// one weighted unit per instance (per valid bucket), plus the
    /// per_type data as one bucket (14.4.2's "sum of all valid buckets
    /// for all the per_instance items +1 (the per_type bucket)"), whose
    /// grade is the weighted average of the per_type items' grades. `None` when
    /// nothing is gradeable.
    pub fn grade(&self) -> Option<f64> {
        // The same weighted per-type aggregate participates whether or
        // not this group also contains per-instance basic items.
        let mut per_type_sum = 0.0;
        let mut per_type_weight = 0.0;
        for item in self
            .items
            .iter()
            .filter(|item| !item.options.hidden && !item.per_instance)
        {
            if let Some(grade) = item.grade() {
                per_type_sum += grade * item.weight as f64;
                per_type_weight += item.weight as f64;
            }
        }
        for item in self.derived.iter().filter(|item| !item.options.hidden) {
            if let Some(grade) = item.grade() {
                per_type_sum += grade * item.weight as f64;
                per_type_weight += item.weight as f64;
            }
        }
        let per_type_grade = (per_type_weight > 0.0).then(|| per_type_sum / per_type_weight);
        let mut weighted_sum = 0.0;
        let mut weight_total = 0.0;
        let mut has_instances = false;
        for item in self
            .items
            .iter()
            .filter(|item| !item.options.hidden && item.per_instance)
        {
            for bucket in item
                .buckets
                .iter()
                .filter(|bucket| !bucket.excluded && bucket.goal > 0)
            {
                has_instances = true;
                let grade = if bucket.hits >= bucket.goal { 1.0 } else { 0.0 };
                weighted_sum += grade * item.weight as f64;
                weight_total += item.weight as f64;
            }
        }
        if !has_instances {
            return per_type_grade;
        }
        if let Some(grade) = per_type_grade {
            weighted_sum += grade;
            weight_total += 1.0;
        }
        (weight_total > 0.0).then(|| weighted_sum / weight_total)
    }

    /// The per-instance units (14.4.2) of the group's `per_instance`
    /// items, in item-then-bucket order.
    pub fn instances(&self) -> Vec<CoverInstanceData> {
        let mut instances = Vec::new();
        for item in &self.items {
            if item.options.hidden || !item.per_instance {
                continue;
            }
            for bucket in item.buckets.iter().filter(|bucket| !bucket.excluded) {
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

    /// Record a basic item and its dependent transitions. Call
    /// `begin_sample` before the round and `record_crosses` afterwards.
    /// Library callers manage expression filters; the interpreter evaluates
    /// them in owner frames at each produced source observation.
    pub fn sample(&mut self, item_name: &str, value: &crate::interp::Value) {
        self.sample_basic(item_name, value);
        self.record_transitions_from(item_name);
        self.refresh_derived();
    }

    pub(crate) fn sample_basic(&mut self, item_name: &str, value: &crate::interp::Value) {
        let Some(item) = self.items.iter().find(|item| item.name == item_name) else {
            return;
        };
        let as_int = value_to_int(value);
        let index = as_int.and_then(|value| {
            item.buckets
                .iter()
                .position(|bucket| value >= bucket.lo && value <= bucket.hi)
        });
        let bucket = if item.buckets.is_empty() {
            BucketIdentity::Unbucketed
        } else {
            index.map_or(BucketIdentity::Outside, BucketIdentity::Basic)
        };
        let label = item_label(item, as_int).unwrap_or_else(|| render_value(value));
        self.remember(
            item_name,
            Observation {
                scalar: ScalarSample::capture(value),
                label,
                bucket,
            },
        );
        let item = self
            .items
            .iter_mut()
            .find(|item| item.name == item_name)
            .expect("sampled item exists");
        item.samples += 1;
        if let Some(index) = index {
            item.buckets[index].hits += 1;
        } else {
            item.ungradeable_hits += 1;
        }
    }

    fn remember(&mut self, name: &str, observation: Observation) {
        if let Some(previous) = self.last_values.insert(name.to_owned(), observation) {
            self.prior_values.insert(name.to_owned(), previous);
        } else {
            self.prior_values.remove(name);
        }
        self.round_items.insert(name.to_owned());
    }

    pub(crate) fn transitions_from(&self, source: &str) -> Vec<usize> {
        self.derived
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                item.kind == CoverItemKind::Transition
                    && item.sources.first().is_some_and(|name| name == source)
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn record_transitions_from(&mut self, source: &str) {
        let mut pending: BTreeSet<_> = self.transitions_from(source).into_iter().collect();
        let mut seen = BTreeSet::new();
        while let Some(index) = pending.pop_first() {
            if !seen.insert(index) {
                continue;
            }
            if let Some(name) = self.record_transition(index) {
                pending.extend(self.transitions_from(&name));
            }
        }
    }

    /// Scalar inputs to a ready derived observation. Composite inputs are
    /// omitted; validation admits only constant predicates over those nodes.
    pub(crate) fn derived_bindings(
        &self,
        index: usize,
    ) -> Option<Vec<(String, crate::interp::Value)>> {
        let item = self.derived.get(index)?;
        if self.round_items.contains(&item.name)
            || !item
                .sources
                .iter()
                .all(|source| self.round_items.contains(source))
        {
            return None;
        }
        let mut bindings = Vec::new();
        for source in &item.sources {
            let current = self.last_values.get(source)?;
            if let Some(value) = &current.scalar {
                bindings.push((source.clone(), value.value()));
            }
            if item.kind == CoverItemKind::Transition {
                let previous = self.prior_values.get(source)?;
                if let Some(value) = &previous.scalar {
                    bindings.push((format!("prev_{source}"), value.value()));
                }
            }
        }
        Some(bindings)
    }

    pub(crate) fn record_transition(&mut self, index: usize) -> Option<String> {
        let item = self.derived.get(index)?;
        if item.kind != CoverItemKind::Transition
            || !item.collect
            || self.round_items.contains(&item.name)
        {
            return None;
        }
        let source = item.sources.first()?;
        if !self.round_items.contains(source) {
            return None;
        }
        let previous = self.prior_values.get(source)?.clone();
        let current = self.last_values.get(source)?.clone();
        Some(self.record_derived(index, vec![previous, current]))
    }

    pub(crate) fn record_cross(&mut self, index: usize) -> Option<String> {
        let item = self.derived.get(index)?;
        if item.kind != CoverItemKind::Cross
            || !item.collect
            || self.round_items.contains(&item.name)
            || !item
                .sources
                .iter()
                .all(|source| self.round_items.contains(source))
        {
            return None;
        }
        let observations: Option<Vec<_>> = item
            .sources
            .iter()
            .map(|source| self.last_values.get(source).cloned())
            .collect();
        Some(self.record_derived(index, observations?))
    }

    fn record_derived(&mut self, index: usize, observations: Vec<Observation>) -> String {
        let labels: Vec<_> = observations
            .iter()
            .map(|value| value.label.clone())
            .collect();
        let buckets = observations.into_iter().map(|value| value.bucket).collect();
        let item = &mut self.derived[index];
        let rendered = render_combination(item.kind, &labels);
        let label = if item.kind == CoverItemKind::Transition {
            format!("({rendered})")
        } else {
            rendered
        };
        let bucket = BucketIdentity::Derived(item.record(labels, buckets));
        let name = item.name.clone();
        self.remember(
            &name,
            Observation {
                scalar: None,
                label,
                bucket,
            },
        );
        name
    }

    /// Recompute nested validity and possible-combination counts in declaration
    /// order. Stable key indices retain history across changing unit masks.
    pub(crate) fn refresh_derived(&mut self) {
        let mut summaries: BTreeMap<String, SourceSummary> = self
            .items
            .iter()
            .map(|item| {
                let valid: Vec<_> = item.buckets.iter().map(|bucket| !bucket.excluded).collect();
                let total = (!valid.is_empty())
                    .then(|| valid.iter().filter(|valid| **valid).count() as u64);
                (
                    item.name.clone(),
                    SourceSummary {
                        total,
                        basic: true,
                        valid,
                    },
                )
            })
            .collect();
        for item in &mut self.derived {
            let sources: Option<Vec<_>> = item
                .sources
                .iter()
                .map(|name| summaries.get(name))
                .collect();
            item.total_combos = sources.as_ref().and_then(|sources| {
                if item.kind == CoverItemKind::Transition {
                    let count = sources.first()?.total?;
                    count.checked_mul(count)
                } else if sources.iter().any(|source| source.total == Some(0)) {
                    Some(0)
                } else {
                    sources
                        .iter()
                        .try_fold(1u64, |total, source| total.checked_mul(source.total?))
                }
            });
            if let Some(allowed) = &item.allowed {
                item.total_combos = Some(allowed.len() as u64);
            }
            let mut valid = Vec::new();
            item.samples = 0;
            item.combos.clear();
            for key in &item.keys {
                let retained = item.allowed.as_ref().is_none_or(|allowed| {
                    let coordinates: Option<Vec<_>> = key
                        .buckets
                        .iter()
                        .map(|bucket| match bucket {
                            BucketIdentity::Basic(index) => Some(*index),
                            _ => None,
                        })
                        .collect();
                    coordinates.is_some_and(|coordinates| allowed.contains(&coordinates))
                });
                let allowed = retained
                    && sources.as_ref().is_some_and(|sources| {
                        key.buckets.iter().enumerate().all(|(position, bucket)| {
                            let source = if item.kind == CoverItemKind::Transition {
                                sources.first()
                            } else {
                                sources.get(position)
                            };
                            source.is_some_and(|source| source.accepts(bucket))
                        })
                    });
                valid.push(allowed);
                if allowed {
                    item.samples += key.hits;
                    item.combos
                        .push((render_combination(item.kind, &key.labels), key.hits));
                }
            }
            summaries.insert(
                item.name.clone(),
                SourceSummary {
                    total: item.total_combos,
                    basic: false,
                    valid,
                },
            );
        }
    }

    /// Complete the round's crosses in declaration order, including descendants
    /// of newly produced crosses and transitions. Sources must be fresh.
    pub fn record_crosses(&mut self) {
        for index in 0..self.derived.len() {
            if let Some(name) = self.record_cross(index) {
                self.record_transitions_from(&name);
            }
        }
        self.refresh_derived();
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
        crate::interp::Value::Str(value) => format!("{value:?}"),
        crate::interp::Value::Real(value) => value.to_string(),
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
    /// Report illegal observations immediately instead of at check time.
    pub check_illegal_immediately: bool,
}

impl Default for CoverageConfig {
    fn default() -> Self {
        Self {
            mode: true,
            max_int_buckets: 16,
            at_least: 1,
            check_illegal_immediately: false,
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
            let derived = build_derived(decl, config);
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
                prior_values: BTreeMap::new(),
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
                base.derived.extend(build_derived(decl, config));
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
                    if let Ok(Some(value)) = coverage_weight(entry) {
                        group_weight = Some(value);
                    }
                }
            }
            if group_at_least.is_none() && group_weight.is_none() {
                continue;
            }
            for item in &mut group.derived {
                if !item.explicit_goal
                    && let Some(goal) = group_at_least
                {
                    item.goal = goal;
                }
                if !item.explicit_weight
                    && let Some(weight) = group_weight
                {
                    item.weight = weight;
                }
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
                        && coverage_weight(&entry.using).ok().flatten().is_none()
                    {
                        item.weight = weight;
                    }
                }
            }
        }
        // Extensions can add a source after a derived declaration was
        // first built. Recompute totals against the final group schema.
        for group in &mut groups {
            group.refresh_derived();
        }
        result.push(TypeCoverage {
            type_name: ty.name.clone(),
            groups,
        });
    }
    result
}

/// Synthesize a group's derived items (14.4/14.5) from its recorded
/// cross/transition entries; final graph totals are computed after merging.
fn derived_name(entry: &CoverItemDecl) -> String {
    scan_string_option(&entry.using, "name").unwrap_or_else(|| match entry.kind {
        CoverItemKind::Transition => format!(
            "transition__{}",
            entry.names.first().map_or("", |(name, _)| name)
        ),
        _ => format!(
            "cross_{}",
            entry
                .names
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>()
                .join("__")
        ),
    })
}

fn build_derived(decl: &CoverGroupDecl, config: &CoverageConfig) -> Vec<CoverDerivedData> {
    decl.items
        .iter()
        .filter(|entry| entry.kind != CoverItemKind::Item)
        .map(|entry| CoverDerivedData {
            kind: entry.kind,
            options: item_options(entry).unwrap_or_default(),
            collect: true,
            allowed: None,
            explicit_goal: coverage_goal(&entry.using).ok().flatten().is_some(),
            explicit_weight: coverage_weight(&entry.using).ok().flatten().is_some(),
            goal: coverage_goal(&entry.using)
                .ok()
                .flatten()
                .unwrap_or(config.at_least),
            weight: coverage_weight(&entry.using).ok().flatten().unwrap_or(1),
            keys: Vec::new(),
            name: derived_name(entry),
            sources: entry.names.iter().map(|(name, _)| name.clone()).collect(),
            samples: 0,
            combos: Vec::new(),
            total_combos: None,
        })
        .collect()
}

fn render_combination(kind: CoverItemKind, labels: &[String]) -> String {
    if kind == CoverItemKind::Transition {
        format!("{}→{}", labels[0], labels[1])
    } else {
        format!("({})", labels.join(", "))
    }
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
        // Cross/transition schemas are built separately below.
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
    let weight = coverage_weight(&item.using).ok().flatten().unwrap_or(1);
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
            excluded: false,
            explicit_goal: false,
            label: "FALSE".to_owned(),
            lo: 0,
            hi: 0,
            goal: 1,
            hits: 0,
        },
        CoverBucket {
            excluded: false,
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
            excluded: false,
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
                excluded: false,
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

fn coverage_weight(tokens: &[Token]) -> Result<Option<u64>, crate::diagnostic::Diagnostic> {
    option_value(tokens, "weight")
        .map(|tokens| {
            constant_integer(tokens)
                .ok()
                .and_then(|value| u64::try_from(value).ok())
                .ok_or_else(|| {
                    option_error(
                        tokens,
                        "coverage weights require nonnegative constant u64 values (14.3-14.5)",
                    )
                })
        })
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

pub(crate) fn item_options(
    item: &CoverItemDecl,
) -> Result<ItemOptions, crate::diagnostic::Diagnostic> {
    Ok(ItemOptions {
        ignore: option_expr(&item.using, "ignore")?,
        illegal: option_expr(&item.using, "illegal")?,
        instance_ignore: option_expr(&item.using, "instance_ignore")?,
        instance_illegal: option_expr(&item.using, "instance_illegal")?,
        hidden: hidden_option(&item.using)?,
        when: option_expr(&item.using, "when")?,
        instance_no_collect: option_expr(&item.using, "instance_no_collect")?,
    })
}

pub(crate) fn group_options(
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
                        "per_unit_instance cannot change after instance coverage options (14.2.2)",
                    ));
                }
                options.unit_scope = Some(scope);
            }
            coverage_goal(entry)?;
            coverage_weight(entry)?;
            options.hidden |= hidden_option(entry)?;
            if let Some(expr) = option_expr(entry, "when")? {
                options.when = Some(expr);
            }
            if let Some(expr) = option_expr(entry, "instance_no_collect")? {
                options.instance_no_collect = Some(expr);
                scope_frozen = true;
            }
        }
        if decl.items.iter().any(|item| {
            ["instance_no_collect", "instance_ignore", "instance_illegal"]
                .iter()
                .any(|name| option_value(&item.using, name).is_some())
        }) {
            scope_frozen = true;
        }
    }
    Ok(options)
}

/// Source order makes the derived graph acyclic (14.4/14.5). The
/// expanded scalar-component cap bounds nested rendered tuples, separately
/// from the possible bucket count (which can be much larger or unbounded).
fn validate_source_graph(
    ty: &crate::elab::ElaboratedType,
    group: &str,
) -> Vec<crate::diagnostic::Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut widths = BTreeMap::<String, usize>::new();
    let declarations = ty
        .covers
        .iter()
        .filter(|decl| decl.name == group && !decl.is_also)
        .chain(
            ty.covers
                .iter()
                .filter(|decl| decl.name == group && decl.is_also),
        );
    for item in declarations.flat_map(|decl| &decl.items) {
        let error = |message: String, span: crate::ast::Span| {
            crate::diagnostic::Diagnostic::new("E0224", message, span.0, span.1)
        };
        let (name, width) = if item.kind == CoverItemKind::Item {
            if item.names.len() != 1 {
                diagnostics.push(error(
                    "basic coverage item requires exactly one name (14.3)".to_owned(),
                    item.span,
                ));
            }
            (
                item.names
                    .first()
                    .map_or(String::new(), |(name, _)| name.clone()),
                1,
            )
        } else {
            if (item.kind == CoverItemKind::Cross && item.names.len() < 2)
                || (item.kind == CoverItemKind::Transition && item.names.len() != 1)
            {
                diagnostics.push(error(
                    "cross requires at least two sources and transition requires one (14.4-14.5)"
                        .to_owned(),
                    item.span,
                ));
            }
            let mut width = Some(0usize);
            for (source, span) in &item.names {
                if let Some(source_width) = widths.get(source) {
                    width = width.and_then(|width| width.checked_add(*source_width));
                } else {
                    diagnostics.push(error(format!("coverage source '{source}' must be defined earlier in this group (14.4-14.5)"), *span));
                }
            }
            if item.kind == CoverItemKind::Transition {
                width = width.and_then(|width| width.checked_mul(2));
            }
            let width = width.filter(|width| *width <= 65_536);
            if width.is_none() {
                diagnostics.push(error(
                    "coverage source graph exceeds the 65536 scalar-component limit (14.4-14.5)"
                        .to_owned(),
                    item.span,
                ));
            }
            (derived_name(item), width.unwrap_or(65_536))
        };
        if widths.insert(name.clone(), width).is_some() {
            diagnostics.push(error(
                format!("duplicate coverage item name '{name}' (14.3-14.5)"),
                item.span,
            ));
        }
    }
    diagnostics
}

/// Structural coverage option checks run on the composed type view,
/// so inherited and extended groups use the same scope as execution.
pub(crate) fn validate_types(
    types: &[crate::elab::ElaboratedType],
    defines: &BTreeMap<String, Expr>,
) -> Vec<crate::diagnostic::Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut constants: BTreeSet<String> = defines.keys().cloned().collect();
    for ty in types {
        constants.extend(ty.enumerals.iter().map(|value| value.name.clone()));
    }
    constants.extend(
        [
            "MAX_INT",
            "MIN_INT",
            "MAX_UINT",
            "MAX_LONGINT",
            "MIN_LONGINT",
            "MAX_LONGUINT",
        ]
        .map(str::to_owned),
    );
    for ty in types {
        let mut seen = BTreeSet::new();
        for decl in &ty.covers {
            if !seen.insert(&decl.name) {
                continue;
            }
            diagnostics.extend(validate_source_graph(ty, &decl.name));
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
                if let Err(diagnostic) = coverage_goal(&item.using) {
                    diagnostics.push(diagnostic);
                }
                if let Err(diagnostic) = coverage_weight(&item.using) {
                    diagnostics.push(diagnostic);
                }
                if item.kind == CoverItemKind::Item
                    && let Some(tokens) = option_value(&item.using, "ranges")
                {
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
                if let Ok(effect_options) = item_options(item)
                    && let Some((name, _)) = item.names.first()
                {
                    let mut inputs = BTreeSet::new();
                    if item.kind == CoverItemKind::Item {
                        inputs.insert(name.clone());
                    } else {
                        for (source, _) in &item.names {
                            let basic = ty
                                .covers
                                .iter()
                                .filter(|group| group.name == decl.name)
                                .flat_map(|group| &group.items)
                                .any(|candidate| {
                                    candidate.kind == CoverItemKind::Item
                                        && candidate
                                            .names
                                            .first()
                                            .is_some_and(|(name, _)| name == source)
                                });
                            if basic {
                                inputs.insert(source.clone());
                                if item.kind == CoverItemKind::Transition {
                                    inputs.insert(format!("prev_{source}"));
                                }
                            }
                        }
                    }
                    for (effect, per_unit) in [
                        (&effect_options.ignore, false),
                        (&effect_options.illegal, false),
                        (&effect_options.instance_ignore, true),
                        (&effect_options.instance_illegal, true),
                    ] {
                        if let Some(expr) = effect {
                            if per_unit && options.unit_scope.is_none() {
                                diagnostics.push(option_error(&item.using, "instance_ignore/instance_illegal require per_unit_instance (14.3.4)"));
                            } else if let Err(diagnostic) = validate_effect(
                                expr,
                                &inputs,
                                item.kind != CoverItemKind::Item,
                                per_unit,
                                &constants,
                            ) {
                                diagnostics.push(diagnostic);
                            }
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
            group.derived.retain(|item| !item.options.hidden);
            group.items.retain(|item| !item.options.hidden);
            for item in &mut group.items {
                item.buckets.retain(|bucket| !bucket.excluded);
            }
        }
    }
    result
}

fn validate_effect(
    expr: &Expr,
    allowed: &BTreeSet<String>,
    derived: bool,
    per_unit: bool,
    constants: &BTreeSet<String>,
) -> Result<(), crate::diagnostic::Diagnostic> {
    let invalid = || {
        crate::diagnostic::Diagnostic::new(
            "E0224",
            if derived {
                "derived coverage predicates require scalar sources, constants and permitted inst fields (14.4-14.5)"
            } else {
                "ignore/illegal predicates require their item, constants and permitted inst fields (14.3)"
            },
            expr.span().0,
            expr.span().1,
        )
    };
    match expr {
        Expr::Ident { name, .. }
            if allowed.contains(name)
                || constants.contains(name)
                || (per_unit && name == "inst") =>
        {
            Ok(())
        }
        Expr::IntLiteral { .. }
        | Expr::BitLiteral { .. }
        | Expr::RealLiteral { .. }
        | Expr::StringLiteral { .. }
        | Expr::BoolLiteral { .. } => Ok(()),
        Expr::Paren { expr, .. } | Expr::Unary { expr, .. } => {
            validate_effect(expr, allowed, derived, per_unit, constants)
        }
        Expr::Binary { lhs, rhs, .. } => {
            validate_effect(lhs, allowed, derived, per_unit, constants)?;
            validate_effect(rhs, allowed, derived, per_unit, constants)
        }
        Expr::Ternary {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            validate_effect(cond, allowed, derived, per_unit, constants)?;
            validate_effect(then_expr, allowed, derived, per_unit, constants)?;
            validate_effect(else_expr, allowed, derived, per_unit, constants)
        }
        Expr::MemberAccess { object, .. } if per_unit && inst_path(object) => {
            validate_effect(object, allowed, derived, per_unit, constants)
        }
        Expr::Index { base, index, .. } if per_unit && inst_path(base) => {
            validate_effect(base, allowed, derived, per_unit, constants)?;
            validate_effect(index, allowed, derived, per_unit, constants)
        }
        Expr::ListConcat { items, .. } => {
            for expr in items {
                validate_effect(expr, allowed, derived, per_unit, constants)?;
            }
            Ok(())
        }
        Expr::InSet { element, set, .. } => {
            validate_effect(element, allowed, derived, per_unit, constants)?;
            for expr in inset_expressions(set) {
                validate_effect(expr, allowed, derived, per_unit, constants)?;
            }
            Ok(())
        }
        _ => Err(invalid()),
    }
}

fn inst_path(expr: &Expr) -> bool {
    match expr {
        Expr::Ident { name, .. } => name == "inst",
        Expr::MemberAccess { object, .. }
        | Expr::Index { base: object, .. }
        | Expr::Paren { expr: object, .. } => inst_path(object),
        _ => false,
    }
}

fn inset_expressions(set: &crate::ast::InSetExpr) -> Vec<&Expr> {
    match set {
        crate::ast::InSetExpr::Items(items) => items.iter().collect(),
        crate::ast::InSetExpr::Range(lo, hi) => vec![lo, hi],
        crate::ast::InSetExpr::Expr(expr) => vec![expr],
    }
}

pub(crate) fn uses_item(expr: &Expr, item: &str) -> bool {
    match expr {
        Expr::Ident { name, .. } => name == item,
        Expr::Paren { expr, .. }
        | Expr::Unary { expr, .. }
        | Expr::MemberAccess { object: expr, .. } => uses_item(expr, item),
        Expr::Binary { lhs, rhs, .. } => uses_item(lhs, item) || uses_item(rhs, item),
        Expr::Index { base, index, .. } => uses_item(base, item) || uses_item(index, item),
        Expr::Ternary {
            cond,
            then_expr,
            else_expr,
            ..
        } => uses_item(cond, item) || uses_item(then_expr, item) || uses_item(else_expr, item),
        Expr::ListConcat { items, .. } => items.iter().any(|expr| uses_item(expr, item)),
        Expr::InSet { element, set, .. } => {
            uses_item(element, item)
                || inset_expressions(set)
                    .iter()
                    .any(|expr| uses_item(expr, item))
        }
        _ => false,
    }
}

fn bare_item(expr: &Expr, item: &str) -> bool {
    match expr {
        Expr::Ident { name, .. } => name == item,
        Expr::Paren { expr, .. } => bare_item(expr, item),
        _ => false,
    }
}

pub(crate) type Intervals = Vec<(i128, i128)>;
pub(crate) type EffectEval<'a> = dyn FnMut(&Expr, Option<i128>) -> Result<crate::interp::Value, crate::diagnostic::Diagnostic>
    + 'a;

/// Exact scalar predicate classification for comparisons/membership and
/// Boolean compositions, with bounded enumeration for other pure shapes.
pub(crate) fn effect_intervals(
    expr: &Expr,
    item: &str,
    domain: (i128, i128),
    prototype: &crate::interp::Value,
    budget: &mut usize,
    evaluate: &mut EffectEval<'_>,
) -> Result<Intervals, crate::diagnostic::Diagnostic> {
    use crate::{
        ast::{BinaryOp, UnaryOp},
        interp::{
            Value, canonical_set, complement_interval_set, intersect_interval_sets,
            union_interval_sets,
        },
    };
    let error = |message| {
        crate::diagnostic::Diagnostic::new("E0402", message, expr.span().0, expr.span().1)
    };
    let boolean = |value: Value| match value {
        Value::Bool(value) => Ok(value),
        _ => Err(error(
            "coverage ignore/illegal predicate expects a Boolean value (14.3)",
        )),
    };
    let scalar = |value: Value| match (prototype, value) {
        (Value::Int(_), Value::Int(value)) => Ok(value),
        (Value::Bool(_), Value::Bool(value)) => Ok(i128::from(value)),
        _ => Err(error(
            "coverage effect comparison requires compatible scalar values (14.3)",
        )),
    };
    if !uses_item(expr, item) {
        return Ok(if boolean(evaluate(expr, None)?)? {
            vec![domain]
        } else {
            Vec::new()
        });
    }
    match expr {
        Expr::Paren { expr, .. } => {
            return effect_intervals(expr, item, domain, prototype, budget, evaluate);
        }
        Expr::Unary {
            op: UnaryOp::Not,
            expr,
            ..
        } => {
            return Ok(complement_interval_set(
                &effect_intervals(expr, item, domain, prototype, budget, evaluate)?,
                domain,
            ));
        }
        Expr::Binary { op, lhs, rhs, .. }
            if matches!(
                op,
                BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::Imply
            ) =>
        {
            let left = effect_intervals(lhs, item, domain, prototype, budget, evaluate)?;
            let active = if *op == BinaryOp::LogicalOr {
                complement_interval_set(&left, domain)
            } else {
                left.clone()
            };
            let mut right = Vec::new();
            for subdomain in active {
                right = union_interval_sets(
                    &right,
                    &effect_intervals(rhs, item, subdomain, prototype, budget, evaluate)?,
                );
            }
            return Ok(match op {
                BinaryOp::LogicalAnd => right,
                BinaryOp::LogicalOr => union_interval_sets(&left, &right),
                _ => union_interval_sets(&complement_interval_set(&left, domain), &right),
            });
        }
        Expr::Binary { op, lhs, rhs, .. }
            if matches!(
                op,
                BinaryOp::Eq
                    | BinaryOp::NotEq
                    | BinaryOp::Lt
                    | BinaryOp::Le
                    | BinaryOp::Gt
                    | BinaryOp::Ge
            ) =>
        {
            let pair = if bare_item(lhs, item) && !uses_item(rhs, item) {
                Some((rhs.as_ref(), true))
            } else if bare_item(rhs, item) && !uses_item(lhs, item) {
                Some((lhs.as_ref(), false))
            } else {
                None
            };
            if let Some((constant, left)) = pair {
                if matches!(prototype, Value::Bool(_))
                    && !matches!(op, BinaryOp::Eq | BinaryOp::NotEq)
                {
                    return Err(error(
                        "Boolean coverage effects support equality comparisons (14.3)",
                    ));
                }
                let value = scalar(evaluate(constant, None)?)?;
                let set = match (op, left) {
                    (BinaryOp::Eq, _) => vec![(value, value)],
                    (BinaryOp::NotEq, _) => complement_interval_set(&[(value, value)], domain),
                    (BinaryOp::Lt, true) | (BinaryOp::Gt, false) => value
                        .checked_sub(1)
                        .map(|hi| vec![(i128::MIN, hi)])
                        .unwrap_or_default(),
                    (BinaryOp::Le, true) | (BinaryOp::Ge, false) => vec![(i128::MIN, value)],
                    (BinaryOp::Gt, true) | (BinaryOp::Lt, false) => value
                        .checked_add(1)
                        .map(|lo| vec![(lo, i128::MAX)])
                        .unwrap_or_default(),
                    _ => vec![(value, i128::MAX)],
                };
                return Ok(intersect_interval_sets(&set, &[domain]));
            }
        }
        Expr::InSet { element, set, .. }
            if bare_item(element, item)
                && !inset_expressions(set)
                    .iter()
                    .any(|expr| uses_item(expr, item)) =>
        {
            let set = match set {
                crate::ast::InSetExpr::Items(items) => {
                    let mut values = Vec::new();
                    for expr in items {
                        let value = scalar(evaluate(expr, None)?)?;
                        values.push((value, value));
                    }
                    canonical_set(values)
                }
                crate::ast::InSetExpr::Range(lo, hi) => canonical_set(vec![(
                    scalar(evaluate(lo, None)?)?,
                    scalar(evaluate(hi, None)?)?,
                )]),
                crate::ast::InSetExpr::Expr(expr) => match evaluate(expr, None)? {
                    Value::Set(values) => values,
                    Value::List(list) => {
                        let mut values = Vec::new();
                        for value in list.items.borrow().iter() {
                            let value = scalar(value.clone())?;
                            values.push((value, value));
                        }
                        canonical_set(values)
                    }
                    _ => {
                        return Err(error(
                            "coverage membership effects require a set or list (14.3)",
                        ));
                    }
                },
            };
            return Ok(intersect_interval_sets(&set, &[domain]));
        }
        _ => {}
    }
    if domain.1.abs_diff(domain.0) >= *budget as u128 {
        return Err(error(
            "wide arithmetic coverage predicates exceed the 65536 value classification limit (14.3)",
        ));
    }
    let mut result = Vec::new();
    let mut start = None;
    let mut value = domain.0;
    loop {
        *budget -= 1;
        if boolean(evaluate(expr, Some(value))?)? {
            if start.is_none() {
                start = Some(value);
            }
        } else if let Some(lo) = start.take() {
            result.push((lo, value - 1));
        }
        if value == domain.1 {
            break;
        }
        value += 1;
    }
    if let Some(lo) = start {
        result.push((lo, domain.1));
    }
    Ok(result)
}
