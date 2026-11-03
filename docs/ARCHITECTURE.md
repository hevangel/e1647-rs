# Architecture (proposed)

```text
.e source -> lexer -> parser -> AST -> elaborator (member composition)
                                                  |
                          constraint IR (conjunctive linear keep) + interpreter
                                                  |
                          xezim VPI bridge (line protocol; one simulator)
```

Still ahead of the code: a typed IR, coverage, an event scheduler, a
multi-simulator HDL API, and wasm32.

## Principles

- **Standard first:** use IEEE 1647 as the semantic reference (subject to lawful access).
- **Independent implementation:** no proprietary runtime reuse or code copying.
- **Determinism:** every randomization test accepts an explicit seed.
- **Testable diagnostics:** keep byte/line/column source spans from the first milestone.
- **Backend separation:** support a CLI/native backend before attempting wasm32.
- **No premature claims:** parsing is distinct from elaboration; elaboration is distinct from simulation.

## Current implementation

- `src/lexer.rs` — tokenizer: keyword recognition (`KEYWORDS`, cross-checked
  item-by-item against the MIT-licensed reference grammar in `external/`),
  e operators with longest-match, IEEE 1647-style numerals (sized/unsized
  based literals, `0x`/`0b`, reals, ranges kept intact at `..`), strings,
  comments, and `<'` / `'>` code-segment markers. A marker counts only
  in the first column with no other text on that line (clause 4.1.2);
  prose outside those segments is skipped. A file with no begin marker
  is tokenized as code throughout. Decimal integers take a `K`/`M`
  multiplier, `0c"..."` is a character literal, and non-decimal based
  literals accept MVL digits. A bare `'` is punctuation, so
  `'path.to.signal'` can be parsed later. Reports the first lexical
  error; later stages collect many. MVL literals are tokenized and not
  evaluated. Block comments (`/* */`) are still accepted beyond
  clause 4.1.3.
- `src/ast.rs` — typed AST for declarations (including `type` enumerals,
  `when` subtypes, `simple_port`, `event`, `import` and `define`), fields,
  type expressions, constraint expressions, methods (including a TCM
  `@event` suffix) and statements, all carrying byte/line/column spans.
- `src/parser.rs` — recursive-descent parser for the declared subset.
  Expression precedence follows clause 4.5 (Table 12): C order for the
  arithmetic and boolean operators, with `^` looser than `|`, `===`
  looser than `==`, the `~` / `!~` string-match operators between `===`
  and `in`, and `is [not] a <type>` between `==` and the comparisons. A
  bare `is` that does not start a type check is left for the caller. A
  user-written `extend global` is rejected (`E0116`, clause 4.3.1.1);
  the parser's file-level `define`/`import` pseudo-declaration is
  unaffected. Error recovery is at declaration,
  member and statement boundaries. The statement subset includes braced `if`/`while`,
  `for each` and `for i from .. to`, `case`, `check that`, `wait`, `emit`,
  `start`, sequence `do` and `stop_run`; `repeat [do] { .. } until
  cond;` (20.2.2, the body at least once) and the optional `do` sugar
  on `if`/`while` — including the one-statement `if cond do
  <statement>;` form — round out clause 20. Struct members include `on`
  event blocks (clause 11.2: dotted event path, action block; extension
  is a repeated declaration) and `on <port>$` event-port blocks
  (clause 11.3: the 9.4.1 `$` access operator names the port's event;
  event ports declare as `name: <dir> event_port [is instance];` along
  the simple_port grammar; the 11.3 restriction — no time-consuming
  actions or TCMs in the block — reports `E0210`), and the clause 21 preprocessor directives
  `#ifdef` / `#ifndef` / `#define` / `#undef` in statement, struct-member
  and action positions — flag semantics only: `#define` records the name
  (the replacement text is consumed, not substituted; clause 21.3's
  replacement rules are a later milestone), excluded `#ifdef`/`#ifndef`
  branches are skipped without parsing, `#else` is not parsed, and the
  conditional forms are rejected at module level (`E0117`). Clause 15
  macro definitions (`define <name'category> "match expression" as
  [computed] { body };`) are recorded uninterpreted — header, pattern
  string and raw body tokens (`E0118` when the pattern string is
  missing) — and simple define-as macros expand before parsing:
  `src/macros.rs` compiles the pattern (15.4), matches invocation
  sites with backtracking (15.5.2; literals match one token each,
  non-terminals consume bounded by bracket depth and separators), and
  splices the matched subsequences at the body's parameter references
  (15.6), later definitions taking priority; the body's trailing `;`
  belongs to the body segment, not the expansion. Clause 5 scalar type declarations (`type name: <type>;`)
  record the underlying representation. Clause 8 template declarations
  (`template [package] struct|unit name of <params> [like base] { .. }`)
  record their `<tag'category>` parameters and defaults; formals
  (`<type>`), template references (`deque of int`) and keyed lists
  (`list (key:it) of t`) are TypeExpr variants, and `new [<type>]
  [with { .. }]` parses to an explicit not-supported runtime
  diagnostic. Template instances monomorphize (8.3): a worklist
  seeded from every type's member types materializes one struct per
  distinct instance under its canonical name (`name of (actuals)`,
  `TypeExpr::render`), substituting actuals for formals in field
  types and method signatures, filling trailing parameters from
  defaults (defaults referencing earlier formals substitute through),
  and substituting the parameterized base to the view's base chain —
  template-typed fields then generate and dispatch through the view.
  Method-body references to other instances and category-compliance
  checks substitute/verify in a later milestone.
  Clause 14 coverage groups (`cover <event> is [also] { .. };`, `is
  empty;`) record their item/cross/transition entries structurally and
  every option clause verbatim; the derived entries synthesize
  `CoverDerivedData` — `transition__item` collects the base item's
  previous→current bucket-label pairs (14.4: n samples → n-1
  transitions) and `cross_a__b` records every combination of the
  crossed items' labels (14.3), each graded over its sources' bucket
  cross-product when all sources have buckets.
  Clause 11.4 `expect`/`assume` members parse their temporal
  expressions into the clause 12 grammar (`TemporalExpr`: event atoms,
  `cycle`, propositions and edge samples, `{;}` sequences, `=>` yield,
  `and`/`or`/`not`/`fail`/`eventually`, repeats, and the binary
  sampling `te @event`; `detach`/`delay`/`exec` report `E0120`) with
  the `else` failure body parsed as actions. Clause 10
  `select { weight : choice; .. }` distributions parse — weights
  structurally, choices verbatim — and the rejection sampler applies
  them (10.2.12.2): each `field == select { .. }` leaf draws an arm
  with probability weight/(sum of all weights) over the field's
  current (already-narrowed) range, and `min`/`max`/`edges`/`others`/
  `normal(center, stddev)` map the choice to a value that narrows the
  field to a point. Choice arguments evaluate as constant integer
  expressions over the recorded tokens (literals, `MAX_UINT`/`MAX_INT`,
  `+`/`-`/`*`/`/`, parentheses); unevaluable choices leave the field
  unwidened. In action position the select still reports the honest
  not-evaluated diagnostic. Predefined
  non-scalar types `file` and `event` are accepted in type positions;
  `for each <type> (v) in list` records the element type filter.
  `package`/`private` member prefixes are recorded on fields.
  Diagnostics
  are collected (codes
  `E01xx` syntax, `E02xx` well-formedness; see the module documentation)
  and the AST stays partial under errors.
- `src/temporal.rs` — the clause 12 temporal-engine core, a pure
  library the interpreter will drive: `sampled_normal_form` propagates
  a temporal expression's sampling event so every atom carries it
  (12.1.4; sub-sampled subtrees keep their own event, 12.2.5) and
  `holds_tightly`/`succeeds` evaluate tight satisfaction (12.1.1.2) and
  last-point success (12.1.1.12) over explicit finite paths of
  `State`s — each state records which events occurred. Operators follow
  their subclauses: sequences over strictly consecutive segments
  (12.2.8), yield as a universal check over the lhs's successful
  prefixes with vacuous hold when it fails (12.2.13), fixed and
  true-match repeats (12.2.9–12.2.10), first-match repeats as sequence
  items (12.2.11), `fail`/`not`/`eventually` (12.2.12/16/14).
  Propositions and repeat bounds are evaluated through caller-supplied
  callbacks; `change/rise/fall(exp)` edge samples report
  `EvalError::EdgeSample` (no sampled value history in the pure core)
  and unresolvable constructs report `EvalError::Unsupported`. The
  same module hosts the run-loop monitoring helpers:
  `succeeded_somewhere` (12.3.1 at any point), `rule_failed` (the
  11.4/12.3.2 expect/assume failure reading over a finished run log —
  yields fail on unfulfilled obligations, `eventually`/success-required
  shapes fail when they never succeeded, `not`/`fail` shapes are
  invariants) and `collect_exprs` (the proposition/bound expressions a
  caller pre-evaluates). Nothing here touches simulation time itself —
  the interpreter feeds it the log.
- `src/check.rs` — minimal well-formedness checks (duplicate
  declaration/field/parameter/local/enumeral names, unknown type names,
  `keep` identifier roots, zero widths). The CLI runs this once on the
  import-merged declaration list. **Not** a symbol/type checker: no type
  system and no subtype-aware scope.
- `src/elab.rs` — member composition: `like` chains and `extend` blocks fold
  into one composed member list per `struct`/`unit` (inheritance-cycle,
  cross-declaration duplicate-field and unmodified-method-redefinition
  diagnostics; codes `E03xx`). Also records `simple_port` HDL paths,
  `on` members (base types first, extends in declaration order),
  recovered `sequence` declarations, and a `type` declaration's
  underlying representation. `when` subtypes (6.6) resolve: each
  block (`when BUSY'kind`, the corpus's space form `when
  NO_CONSTRAINT env` — two bare modifiers, value first, field id
  second, or a chained `when LO'kind HI'sel`) synthesizes a subtype
  view named with its values space-joined before the base type
  (`LO HI pkt`) with `when_of` carrying every (field, value) pair —
  the base's composed members with the when block's fields,
  constraints, methods, `on` blocks, cover groups and expects
  appended. A `when` member nested inside a when block applies on top
  of the enclosing view (a further combined view; a worklist applies
  nested levels until none are left, budget-capped at 200 synthesized
  views). When-qualified field types (`TypeExpr::WhenQualified`,
  `f: LO pkt` or chained `f: LO HI pkt` — parsed as nested
  qualifiers) instantiate the view, so those members apply for the
  instance and `is a` walks the view's base chain. **Not** full
  elaboration: base-typed instances do not dynamically match
  subtypes; no type checking.
- `src/builtins.rs` — predeclared names the executed subset relies on:
  the `sys` unit, sequence bases (`any_sequence_item` and siblings), and
  routines such as `out`, `outf`, `message`, `pack`, `append`,
  `dut_error`, `stop_run`, the objection no-ops, and the `hdl_*` bridge
  calls; the predeclared type names `file`, `event` and `set` (5.1.10).
- `src/coverage.rs` — the clause 14 coverage data model, built from
  the recorded cover groups at run start: `is also`/`empty`
  extensions merge into their base group by name (14.5, either
  declaration order), `using ranges = { range([lo..hi], "label");
  ... }` clauses parse into explicit buckets, and items without them
  take the 14.2 defaults (16 buckets for integers — "0 through 15" —
  two for booleans; anything else is ungradeable with a note). The
  interpreter samples a group when its event fires: each item's
  recorded expression evaluates in the emitting frame (or the named
  field is read) and the value lands in the containing bucket;
  out-of-range values are ungradeable (14.2: "goals of 0"). Grades
  are covered-buckets-per-item, averaged over gradeable items; cross/
  transition items grade over their sources' bucket cross-product.
  The option tokens apply where the prose names them: `at_least = N`
  is the per-bucket goal, `per_instance = TRUE` (item or group
  spelling) makes each bucket its own graded instance (14.2.1 —
  `CoverInstanceData`, name `item==bucket`, holes for unsampled
  buckets, the group grade = the weighted instance grades plus the
  per_type bucket) and `weight = N` scales the grade contribution;
  the coverage configuration of 28.9 and `range(...)`'s numeric
  parameters beyond bounds/label are recorded, not applied.
- `src/interp.rs` — interpreter for the statement subset (codes
  `E0401`–`E0410`). Entry: the built-in `sys` unit's `run()` methods, with
  and named types by declared name, walking `like` bases — subtype-aware
  dispatch is a later milestone. String matching evaluates only the
  case-insensitive full-string literal subset; `/.../` AWK patterns and
  native meta-characters report `E0410`. `gen` uses the
  constraint-solver backend
  when that compiles in, and rejection sampling otherwise — the sampler
  narrows each field's domain to the intersection of the type's constant
  range/equality constraints before sampling. `start` runs
  the called method inline; `wait` advances a virtual cycle counter and
  does not drive the simulator; `emit` records a firing and executes the
  event's `on` action blocks inline, in composition order, on the
  instance the event path resolves to (clause 11.2) — `@sim`-attached
  events never fire. `emit` also records the occurrence in the run's
  temporal log (one `State` per virtual cycle, clause 12). The clause
  13.1.1 scheduler runs concurrent TCMs: the `run()` bodies are the
  main coroutine and every `start`ed TCM joins the queue with the
  implicit sync cycle before it commences; coroutines take round-robin
  turns that run atomically from one synchronization action to the
  next, and when nothing is eligible one cycle passes (the 5,000-cycle
  shared budget guards runaway programs, `E0409`). Suspending
  captures the locals frame and builds a skip-cursor over `while`
  chains — resumption re-enters the mid-iteration loop without
  re-checking its condition and re-executes the wait statement
  against the grown log (13.1.1(b)); `compute` and inline calls are
  atomic regions whose waits spin inline, as do waits inside
  `if`/`case`/`for` bodies (the documented v1 limitation of the
  cursor). Temporal waits (`@event`, sequences, propositions, event
  repeats, with 13.1.3's readability-only `until`) re-evaluate at
  every turn (12.3.1); propositions and repeat bounds evaluate with
  current field values (no per-cycle value history); a temporal wait
  unsatisfied after 1,000 cycles reports `E0409`, and unevaluable TEs
  (`change/rise/fall`, unresolvable bounds) report `E0402` honestly.
  `sync` (13.1.2) parses in bare and TE forms — the bare form
  suspends one cycle, the TE form behaves like the temporal wait.
  `expect`/`assume` rules (elaborated per type, 11.4) are monitored at
  quit with `rule_failed` (12.3.2): a failed rule runs its `else` body
  on the sys instance as a check error, or prints the failure when
  there is no else body. One global timeline — no per-instance
  sampling. `emit` also samples the cover groups (clause 14) of the
  instance whose event fired, recording item values into the
  `src/coverage.rs` buckets; `run --coverage` prints the report.
  `gen <obj>` gates the object with the enclosing instance's
  constraints that reference it (10.2.7's connected field sets, v1):
  depth-two paths rebase (`p.v == select { .. }` → `v == select
  { .. }`) so when-view distribution policies drive generation;
  constraints that do not rebase (other roots, deeper paths,
  whole-object equalities) drop until full CFS scoping lands. The
  reflection set (5.8.4/27.4) evaluates: `set_of_values(type)` builds
  a `Value::Set` — canonical inclusive intervals (sorted, merged,
  non-neighboring) over the type's width/signedness or enumeral
  indices — and the set pseudo-methods (`size`/`uint_size`,
  `min`/`uint_min`/`int_min` and the `max`/`uint_max`/`int_max`
  mirrors, the `_is_uint` guards, `union`/`intersect`/`diff`) run on
  set values; `to_string()` renders scalars and sets, and `as_a` to
  an enumeral type maps the integer index to the enumerand.
- `src/solve.rs` — constraint IR and the pluggable solver seam: the
  top-level `keep` constraints of one type translate (conservatively) into
  bounded-integer variables plus a conjunction of linear comparisons;
  range membership (`x in [lo..hi]`) translates to the two comparisons,
  item-list membership does not (no disjunction in the IR). Backend:
  Pumpkin (pure-Rust CP solver) behind the `solver-pumpkin`
  cargo feature — seeded, deterministic; without the feature `solve`
  reports unsupported and `gen` falls back to rejection sampling.
- `src/hdl.rs` — one SystemVerilog binding. `HdlSession` spawns `xezim`
  with `c/xezim_bridge.c` and exchanges `SET` / `GET` / `RUN` / `FINISH`
  lines. Arguments are allowlisted; there is no shell. Quoted HDL paths
  and the `hdl_*` routines use this session.
- `src/main.rs` — `e1647 tokens`, `e1647 parse`, `e1647 elab` and
  `e1647 run [--seed N]`. `import a.b` loads `a/b.e` or `a.b.e` next to
  the source and splices those declarations in front. `parse`, `elab` and
  `run` print diagnostics to stderr
  (`path:line:col: error[code]: message`) and their results on success.

Deliberately not implemented yet: dynamic subtype matching for
base-typed instances (when views resolve through when-qualified field
types; chained modifiers and nested when-in-when blocks compose,
though a view's own when-of pairs are not re-verified against its
base's fields), non-linear,
disjunctive or temporal constraint solving (the solver backend covers
conjunctive linear constraints and constant ranges over bounded
integers), `@`-triggered TCM invocation, `@sim` sampling and
per-instance timelines, and suspension re-entry beyond `while` chains
(waits inside `if`/`case`/`for` bodies, inline calls, `compute` and
`on` blocks spin inline instead of interleaving), the remaining
clause 14 coverage model (basic and derived items collect and grade;
per_instance items grade per bucket with holes and the weighted
group grade; the option tables beyond per_instance/at_least/weight
and the 28.9 configuration are recorded, not applied), integer width
truncation, evaluation of MVL literals, `detach`/`delay`/`exec` TEs, select
choice policies beyond the constant-expression subset (the full
choice-expression grammar, soft-order interactions between multiple
selects), HDL-edge- and bind()-driven event-port triggering (the
`on p$` members run on `emit p$`; the simulator-side triggers ride
the `@sim` gap), `#else` and `#define` replacement
rules, `template extend` (8.2) and method-body instance
substitution, template category-compliance checks (8.3's notes),
computed macros and the match-expression grammar's repetition and
category validation (clause 15.3/15.4 — simple define-as macros
expand), the Part 2 routine library beyond the reflection set
(`set_of_values`/`full_set_of_values` and the 27.4 set
pseudo-methods work; set literals, range modifiers on declared
types, and bare scalar keywords as type arguments do not), a multi-simulator HDL
API, and WASM.
