# Architecture (proposed)

```text
.e source -> lexer -> parser -> AST -> elaborator (member composition)
                                                  |
                          constraint IR (conjunctive linear keep) + interpreter
                                                  |
                          xezim VPI bridge (line protocol; Simulator backend)
```

Still ahead of the code: a typed IR, and simulator bindings beyond the
xezim bridge (VHPI among them). The library compiles for wasm32; a
browser sandbox API is not started.

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
  `#ifdef` / `#ifndef` / `#define` / `#undef` / `#else` in statement, struct-member
  and action positions — `#define` records the flag and the 21.3
  replacement rules apply as a token pre-pass (`src/preproc.rs`, before
  the clause 15 macro expansion per B.5.1's two phases): the
  replacement text substitutes for every occurrence of the name in
  subsequent code, except inside string literals and the directives
  themselves, excluded branches define nothing and are skipped without
  parsing (21.2), and `#else` flips the included side — both
  `} #else { .. };` and `}; #else { .. };` spellings (the 21.2 syntax
  examples are images in the local redline copy, so only the prose is
  implemented; substitution is single-pass and per-file, and no
  `#endif` is accepted). The conditional forms are rejected at module
  level (`E0117`). Clause 15
  macro definitions (`define <name'category> "match expression" as
  [computed] { body };`) are recorded — header, pattern string and raw
  body tokens (`E0118` when the pattern string is missing) — and both
  kinds expand before parsing: `src/macros.rs` compiles the pattern
  (15.4), matches invocation sites with backtracking (15.5.2; literals
  match one token each, non-terminals consume bounded by bracket depth
  and separators), and either splices the matched subsequences at the
  simple body's parameter references (15.6) or evaluates the computed
  body (15.3) with a restricted parse-time interpreter — `var`
  declarations and assignments, braced `if`/`else`, `return`, the
  implicit `result` variable, string parameters (their matched text),
  string literals, `+` concatenation, `==`/`!=` comparisons and calls —
  where `reject_match()` and any unevaluable construct (method calls,
  `is a` checks, `else if` chains, list/number expressions, unknown
  routines, non-string results) decline the definition so the next
  lower-priority definition applies (15.5.1), the returned string
  tokenizing into the expansion; later definitions take priority, and
  the body's trailing `;` belongs to the body segment, not the
  expansion. Clause 5 scalar type declarations (`type name: <type>;`)
  record the underlying representation. Clause 8 template declarations
  (`template [package] struct|unit|extend name of <params> [like base]
  { .. }`) record their `<tag'category>` parameters and defaults; a
  `template extend` (8.2) is flagged `template_extend` with its own
  parameter list instead of being folded by name; formals
  (`<type>`), template references (`deque of int`) and keyed lists
  (`list (key:it) of t`) are TypeExpr variants. Template instances
  monomorphize (8.3): a worklist
  seeded from every type's member types and method bodies
  materializes one struct per
  distinct instance under its canonical name (`name of (actuals)`,
  `TypeExpr::render`), substituting actuals for formals in field
  types, method signatures and method bodies (the type positions
  inside actions — `new <template> of <type>`, casts, `is a` checks,
  local types — substitute, and every instance a body names joins the
  worklist), filling trailing parameters from
  defaults (defaults referencing earlier formals substitute through),
  and substituting the parameterized base to the view's base chain —
  template-typed fields then generate and dispatch through the view.
  Each extension applies per instance when every actual complies with
  the extension's parameter specifications (8.3's NOTE 3): the
  prose-named `scalar`/`enumerated` categories check the actual's
  shape (8.1.2's category table is an image, so any other spelling is
  read as 8.2's specific-type form and can only ever mismatch), the
  original template's own categories stay unvalidated (8.3's NOTE 2),
  and an arity mismatch reports `E0304` once and skips the extension.
  Typed `new <named-or-template-instance> [with { .. }]` instantiates
  and runs the `with` assignments (clause 6.2); untyped `new` parses
  to an explicit not-supported runtime diagnostic.
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
  per_type bucket), `weight = N` scales the grade contribution, the
  group-level `at_least`/`weight` spellings apply to items without
  their own (14.8.2/14.8.3), `no_collect`/`instance_no_collect`
  (group, 14.1.1; item, 14.2.3) drop the group/item from the
  collected model when TRUE, and the cross/transition `name` option
  renames the derived item (14.3/14.4). The 28.9 coverage
  configuration (`CoverageConfig`: the coverage mode per 14.1's NOTE
  and `max_int_buckets` per 14.2's bracket) is honored through the
  `build_with_config` builder API — the `set_config()` routine the
  prose names is not wired; `range(...)`'s numeric parameters beyond
  bounds/label are recorded, not applied.
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
  instance the event path resolves to (clause 11.2). `emit` also records
  the occurrence in the run's temporal log (one `State` per virtual
  cycle, clause 12), and the simulator-side triggers fire through the
  same path: `rise/fall/change(HDL expression) @sim` events (11.1.4.2)
  and `hdl_path()`-bound event ports (9.4, 9.7.2.8) are edge-sampled
  every virtual cycle while an HDL source is loaded — a live session or
  the injected signal-history test seam (`hdl::SignalSource::Injected`;
  without one they never fire). The clause
  13.1.1 scheduler runs concurrent TCMs: the `run()` bodies are the
  main coroutine and every `start`ed TCM joins the queue with the
  implicit sync cycle before it commences (a TCM declared `@event` with
  a declared sampling event commences when that event occurs — 17.1.2,
  17.2.1); coroutines take round-robin
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
- `src/hdl.rs` — the simulator seam: the `Simulator` trait
  (`load`/`name`/`set`/`get`/`run`/`finish`), a small explicit registry
  (`registered_backends`, `default_backend`, `load_backend`, `load`),
  and two backends. `XezimSimulator` (native targets) wraps `HdlSession`,
  which spawns `xezim` with `c/xezim_bridge.c` and exchanges `SET` /
  `GET` / `RUN` / `FINISH` lines; arguments are allowlisted and there is
  no shell. `NullSimulator` is the no-op backend where no simulator
  exists — under wasm32 it is the only registered one (the spawning
  backend is not compiled there, so `hdl_load` fails with the honest
  no-simulator error) and it doubles as the tests' dispatch check. A
  third backend (VHPI — not implemented) plugs in with one `Simulator`
  impl and one registry arm. Quoted HDL paths and the `hdl_*` routines
  read through `SignalSource`, which holds whichever backend loaded (or
  the injected per-cycle history).
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
integers), per-instance temporal timelines (the run log stays one
global timeline; the simulator-side `@sim`/event-port triggers fire
per instance at the scheduler's cycle boundaries), and suspension
re-entry beyond `while` chains
(waits inside `if`/`case`/`for` bodies, inline calls, `compute` and
`on` blocks spin inline instead of interleaving), the remaining
clause 14 coverage model (basic and derived items collect and grade;
per_instance items grade per bucket with holes and the weighted
group grade; the group/item `no_collect`/`instance_no_collect`
options, the cross/transition `name` option and the group-level
`at_least`/`weight` apply; the bool-exp option forms with the
implicit `inst` variable, the per-unit-instance scoping
(14.1.1/14.2.3), the `radix`/`when`/`never` option semantics (the
option tables are images) and the `set_config()` routine the 28.9
configuration names are not modeled — the configuration is the
`build_with_config` builder API), integer width
truncation, evaluation of MVL literals, `detach`/`delay`/`exec` TEs, select
choice policies beyond the constant-expression subset (the full
choice-expression grammar, soft-order interactions between multiple
selects), event-port triggering beyond the `hdl_path()`-bound edge
sampling (`on p$` members run on `emit p$` and on the bound object's
edges per the `edge()` attribute — the MVL edge spellings and
`bind()` connections are not modeled), the preprocessor's
rescanned/repeated substitution
and backtick preprocessor names (replacement rules apply in one
pass, per file — B.5's include-order visibility through `import` is
not modeled), clause 8.4's template when-subtype instances, extension
members beyond fields/constraints/methods/`on` blocks and the 8.2
subtype propagation of applied extensions (the 8.3 NOTE 2
actual-category validation stays out with the original template's),
computed macros beyond the evaluated body subset (method calls,
`is a` checks, `else if` chains, list/number expressions — such
bodies decline, clause 15.3) and the match-expression grammar's
repetition and category validation (clause 15.4), the Part 2 routine
library beyond the reflection set
(`set_of_values`/`full_set_of_values` and the 27.4 set
pseudo-methods work; set literals, range modifiers on declared
types, and bare scalar keywords as type arguments do not), simulator
bindings beyond the xezim bridge (the `Simulator` seam is registered;
VHPI among the bindings that are not written), a backend selection knob
(`hdl_load` resolves through the default backend), and a browser
sandbox API (the wasm32 compilation check passes; there is no sandbox
API and no wasm runtime test).
