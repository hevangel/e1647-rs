//! The coverage data model (clause 14): buckets, hits and grades for
//! the cover groups the parser records.
//!
//! Groups are built from the recorded [`CoverGroupDecl`]s at run start:
//! `is also` / `empty` extensions merge into their base group by name
//! (14.5), and each item's `using ranges` clause — recorded as raw
//! tokens in the shape `ranges = { range([lo..hi], "label"); ... }` —
//! is parsed into explicit buckets. Items without a `ranges` clause
//! get the clause
//! 14.2 defaults: 16 buckets for integers ("buckets are created by
//! default for only the first 16 possible values (0 through 15)") and
//! the two values for booleans. Values falling outside every bucket
//! are ungradeable ("those values are given goals of 0 and do not
//! affect the grade for the item").
//!
//! Not modeled yet: `cross`/`transition` items (recorded, not
//! collected), per-instance coverage (14.2.1), the coverage
//! configuration options of 28.9, and the numeric parameters of
//! `range(...)` beyond the bounds and label (their semantics live in
//! Table 41, an image in the local redline copy — they are recorded
//! and counted but not applied).

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
}

impl CoverItemData {
    /// The item's grade (14.2): the fraction of buckets that reached
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

/// A derived coverage item (14.3/14.4): a cross over the group's items
/// or a transition over one item's consecutive samples. Each sampling
/// event records one combination — the crossed items' bucket labels,
/// or the base item's previous→current labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverDerivedData {
    /// The 14.3/14.4 default names: `cross_a__b` and
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
    /// Derived items (crosses and transitions, 14.3/14.4).
    pub derived: Vec<CoverDerivedData>,
    pub samples: u64,
    /// Each item's containing-bucket label (or raw value) at the last
    /// sampling event — the transition source (14.4's previous-value).
    last_values: BTreeMap<String, String>,
}

impl CoverGroupData {
    /// The group's grade: the plain average of the gradeable items'
    /// (basic and derived) grades; `None` when none is gradeable.
    /// (Weighted and per-instance grades are 14.2.1/28.9, not modeled.)
    pub fn grade(&self) -> Option<f64> {
        let mut grades: Vec<f64> = self.items.iter().filter_map(|item| item.grade()).collect();
        grades.extend(self.derived.iter().filter_map(|item| item.grade()));
        if grades.is_empty() {
            return None;
        }
        Some(grades.iter().sum::<f64>() / grades.len() as f64)
    }

    /// Record one sampled value for the named item, then update the
    /// transition/cross derived items (14.4: the transition derives
    /// `previous-value, value` pairs from the base item's consecutive
    /// samples; 14.3: the cross records every combination of the
    /// crossed items' values). The group's sample count is kept by the
    /// caller — once per sampling event, not once per item.
    pub fn sample(&mut self, item_name: &str, value: &crate::interp::Value) {
        let item = self.items.iter().find(|item| item.name == item_name);
        let as_int = value_to_int(value);
        // The bucket label this value lands in (14.2's bucket model),
        // or the raw value when the item is unbucketed.
        let label = item.and_then(|item| item_label(item, as_int));
        let label = label.unwrap_or_else(|| render_value(value));
        // The transition over this item: previous → current (14.4).
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
            // Outside every bucket: ungradeable (14.2).
            item.ungradeable_hits += 1;
        }
    }

    /// The cross pass of one sampling event (14.3): record each cross
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

/// The label of the bucket containing the value (14.2), or `None` when
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

/// Build the coverage data model from the elaborated types' recorded
/// cover groups: one [`TypeCoverage`] per type that declares (or
/// extends) groups, extensions merged into their base by name (14.5),
/// in any declaration order.
pub fn build(elaborated: &crate::elab::Elaborated) -> Vec<TypeCoverage> {
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
            let items: Vec<CoverItemData> = decl.items.iter().filter_map(build_item).collect();
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
                base.items.extend(decl.items.iter().filter_map(build_item));
                base.derived.extend(build_derived(decl, &base.items));
            }
        }
        result.push(TypeCoverage {
            type_name: ty.name.clone(),
            groups,
        });
    }
    result
}

/// Synthesize a group's derived items (14.3/14.4) from its recorded
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
                derived.push(CoverDerivedData {
                    name: format!("transition__{base}"),
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
                derived.push(CoverDerivedData {
                    name: format!("cross_{}", sources.join("__")),
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
/// `using` tokens carry them, the 14.2 defaults otherwise.
fn build_item(item: &CoverItemDecl) -> Option<CoverItemData> {
    if item.kind != CoverItemKind::Item {
        // cross/transition items are recorded, not collected yet.
        return None;
    }
    let name = item
        .names
        .first()
        .map(|(name, _)| name.clone())
        .unwrap_or_default();
    let mut ranges = Vec::new();
    if let Some(note) = parse_ranges(&item.using, &mut ranges) {
        return Some(CoverItemData {
            name,
            buckets: Vec::new(),
            ungradeable_hits: 0,
            samples: 0,
            note: Some(note),
        });
    }
    let (buckets, note) = if ranges.is_empty() {
        // 14.2 defaults: integers grade over the first 16 values;
        // booleans over their two values; anything else has no default
        // bucket set and is ungradeable.
        match &item.ty {
            Some(ty) if type_is_bool(ty) => (bool_buckets(), None),
            Some(ty) if type_is_int(ty) => (default_int_buckets(), None),
            _ => (
                Vec::new(),
                Some("no ranges clause: values are recorded, not graded"),
            ),
        }
    } else {
        (ranges, None)
    };
    Some(CoverItemData {
        name,
        buckets,
        ungradeable_hits: 0,
        samples: 0,
        note,
    })
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

fn default_int_buckets() -> Vec<CoverBucket> {
    (0..16)
        .map(|value| CoverBucket {
            label: format!("{value}"),
            lo: value,
            hi: value,
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

fn type_is_int(ty: &TypeExpr) -> bool {
    matches!(
        ty,
        TypeExpr::Scalar {
            base: ScalarBase::Uint
                | ScalarBase::Int
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
