# Status

**Pre-alpha.** The implementation target is **IEEE 1647-2019** — code
comments, diagnostics and docs cite 2019 clause numbers (historical
verification-log entries cite 2016; see [REFERENCES.md](REFERENCES.md)).
Everything below is backed by tests (`cargo test --all-targets`) and the
verification log; nothing here claims IEEE conformance. This page says
what runs today — the staged plan is [ROADMAP.md](ROADMAP.md), the
designs behind the claims are [ARCHITECTURE.md](ARCHITECTURE.md).

## The pipeline, end to end

- **Interface contracts.** Concrete present prototypes are checked against
  effective implemented interfaces, including inherited/extended contributors.
  Definite parameter/result/arity/static mismatches report E0311. Plain aliases
  use the current representation model; constant widths and short unsigned
  bases normalize. Missing methods remain runtime E0414. Symbolic/ranged types,
  template/conditional-only implementations and full extension/signature
  equivalence remain incomplete.
  Composed method extensions report E0225 for definite contract changes,
  including when views and substituted templates; omitted annotations inherit.
  Static declarations now precede extend contributions even when the extend
  appears first in source. Shared method execution plans implement first/also/only
  ordering and replacement, preserving ancestor wrappers around subtype bodies.
  First wrappers can update scalar arguments for their older subtree; also
  wrappers receive their original arguments and the older result. Returns end
  the current wrapper subtree while later outer also wrappers still run. Typed
  methods have an implicit result slot (reference defaults are NULL), checked
  conservatively for assignments. Atomic/static calls, suspended inline/started
  calls, lifecycle hooks and inline sequence MAIN share these policies. Full
  language execution and reference-simulator qualification remain incomplete.
- **Browser sandbox.** An experimental single-source parse/check/run API and
  local demo execute the default-feature WASM library in disposable workers.
  Structured diagnostics, output, DUT failure counts and basic/derived coverage
  summaries are tested in Chromium. Derived crosses/transitions retain source
  names, goals/weights, domain totals, grades and observed combination arrays,
  including duplicate labels and per-unit ownership. The shipped module has zero host imports,
  a 128 MiB linear-memory cap and host cancellation/deadlines. Imports and
  native HDL are unavailable; other browser engines remain unqualified. See [BROWSER_SANDBOX.md](BROWSER_SANDBOX.md).
- **Front end.** A source tokenizer and a recursive-descent parser for a
  declared subset: struct/unit/extend/type/interface/annotation and
  template declarations, fields, scalar/list/named/enumeral types,
  `keep`/`keep soft` constraints, `when` subtype members, ports, events,
  sequences, and methods with statement bodies. `import` loads a sibling
  file. Member composition folds `like` chains and `extend` blocks into
  per-type member lists. Failures produce spanned diagnostics with
  fixture-tested output (`tests/fixtures/`).
  Glued tick suffixes remain identifiers even when numeric-looking; separate
  complete apostrophe numerals remain numbers. Partial prefixes no longer split
  names, and simple numeric-looking quoted HDL paths keep their quotes. This
  does not extend escaped/indexed HDL path parsing or execute MVL numerals.
- **Elaboration.** The composed types feed the interpreter: `when`
  subtypes synthesize subtype views whose members apply;
  `typeof()`/`typeof_item()` type positions resolve; template instances
  monomorphize (actuals substitute for formals, method bodies included)
  and `template extend` blocks apply to the instances whose actuals
  match; static members register per type. After composition, the
  conservative type check ([`typing`]) walks every method body, field
  init value, `keep` constraint, `on` body, coverage item/Boolean option and synthesized
  `when` view, and reports definite mismatches only: a value that cannot be
  the target it is stored into (E0220), a member a known when-less
  receiver does not have (E0221), indexing a non-list (E0222), an
  operator on incompatible operand kinds (E0223) — anything uncertain
  stays silent. Traversal includes nested membership/casts/slices/tables,
  scoped `evaluate` and `new ... with` bodies, all statement value children,
  and temporal rule/wait proposition and repeat-bound expressions. Explicit
  `evaluate`, bit-slice and `in_table` results retain known kinds. Known
  list indexes require integer/enum kinds; membership checks known container
  and scalar/list element compatibility. A set kind represents declared sets,
  literals and set_of_values results, without tracking scalar domains. Unknown
  receivers/operands stay conservative. General bidirectional inference,
  pre-elaboration type-expression checking and full set-domain typing remain
  incomplete. Retained type annotations traverse width/key/template-value and
  typeof expressions in their enclosing scopes, with an element-typed it for
  keyed lists. Definite non-integer scalar widths report E0223. All parameters
  bind before dependent annotations; type-position hints/bindings stay isolated.
  Explicit is-a bindings retain their matched kinds on proven Boolean paths:
  lazy and/or/implication operands, if branches, while bodies, check failure
  actions and ternary arms. Ambiguous joins and possible replacement bindings
  discard kinds; operand narrowing and general flow-sensitive inference remain
  incomplete.
  Known declaration/assignment/field/parameter/return kinds now contextually
  type untyped evaluate/new bodies. Runtime contexts include indexed/static
  targets, qualified calls, supported inferred-value shapes and return
  metadata saved across TCM suspension. Calls, init actions and expression
  bodies isolate their contexts; missing runtime contexts still report E0122
  for evaluate and E0402 for new. Materialized template kinds propagate and
  concatenation inference flattens list operands like the value model.
  Registered static field accesses and qualified call signatures retain
  kinds; static methods/handlers and initializers are checked once in their
  declaring contexts. Static extensions can inherit omitted annotations
  from their declaration. Bare field writes work without an instance in
  static methods/handlers, including inherited fields; unqualified static
  method calls remain unsupported. Scalar/list/reference alias defaults
  follow their representation with a 64-alias bound (E0402 on excess).
  Declared method parameters bind within their method, and type-inferring
  local declarations retain known initializer kinds (clause 18),
  including homogeneous list literals and equal-kind ternary branches.
  Scalar/list aliases resolve with a recursion bound; custom numeric
  dispatch stays outside the conservative lattice.
- **Execution.** `e1647 run` executes the built-in `sys` unit through
  the Table 1 phases — `setup`, generation (with `pre_generate`/
  `post_generate` and the `connect_pointers`/`connect_ports`/
  `check_generation` sub-phases), the clause 13.2.2 scheduler running
  `start`ed TCMs concurrently, `run`, then `extract` → `check` →
  `quit`/`finalize`. `emit` feeds an event log; temporal waits
  re-evaluate against it; `expect`/`assume` rules stream through
  per-rule monitors ([`temporal::RuleMonitor`]) — at every scheduler
  cycle boundary the monitor consumes the newly finalized states, an
  invariant rule (`not`/`fail`) whose forbidden behavior occurred
  fires at that cycle, and the obligation rules (`=>`, `eventually`)
  report their end-of-run verdicts when their windows close. The
  clause 33 blocking TCMs (`up()`/`down()`/`lock()`) yield at the call
  and park blocked calls on a per-object FIFO wake pass.
  Statement-position method calls and `compute` retain callee frames
  across suspension, including recursive helpers, composed method
  bodies and entry-event synchronization. Receivers and arguments
  are retained across resumption. Scheduled instance calls in statement
  value expressions also suspend: assignments, conditions, arguments,
  receivers, returns, print parts and loop headers retain completed values.
  Lazy branches keep their chosen path; while conditions start fresh after
  each body, and a suspended repeat-until condition resumes after its body.
  Operator/formatting callbacks completed before a later suspension do not
  replay. Runtime-resolved instance receivers use the same call engine.
  Scoped evaluate/constructor actions, list callbacks, generation/coverage
  observation, static methods, on blocks and check failure actions remain
  atomic. Compute arguments receive declared parameter hints,
  including contextual constructors and evaluate expressions. Physical event
  keys distinguish owners and unify bare/dotted/alias references. Emission
  receivers evaluate once, including calls/indexes; on actions and coverage
  sample once. Entry events bind in the callee's owner/parameter scope.
  Static Type::event paths also parse in temporal expressions. Every created
  instance owns rule monitors and expression/failure-action contexts; new
  instances join monitoring, while unused declarations create no monitors.
  Event references bind at wait/rule entry. Active reference rebinding,
  indexed temporal path syntax and full event-specific timelines remain gaps.
  A suspended wait keeps its age across rechecks; successive waits reset it.
- **Membership.** Scalar/list membership (4.11.6) executes over lists and
  integer/Boolean/enumeral sets. List-to-list containment is order-independent
  and requires distinct matches for repeated left values. Iterative matching
  handles overlapping mixed numeric comparisons; operands are evaluated once
  and comparisons use snapshots. Set membership tests each left item without
  consuming set entries. Real-valued set membership remains unsupported.
- **Randomization.** `gen` is a seeded rejection sampler by default;
  for translatable `keep` constraints it solves instead: linear
  problems through the backend conjunction solver (the
  `solver-pumpkin` cargo feature), non-linear polynomial problems
  (`a * b == c`, squares) through a feature-independent bounded
  enumeration solver — disjunctive shapes (`||`, `not`, item-set
  membership) translate in both paths, and every run is deterministic
  per seed within each backend. The CLI regression verifies complete
  nested objects and lists across separate processes for both implicit
  generation and explicit `gen`. `select` distributions
  (weights/min/max/edges/others/normal) bias the sampler.
- **Observability.** Coverage groups (clause 14) collect basic-item
  buckets and grades on their sampling event after
  `set_config(cover, mode, normal)` enables collection. Collection
  starts disabled; `off` pauses it without clearing counts, and
  `count_only` collects counts with no interactive post-processing API.
  `max_int_buckets` accepts 0–65536 before sampling starts; explicit
  ranges and Boolean buckets retain their declared/default shapes.
  `run --coverage` reports collected groups; `print`/`out` and the
  message hooks produce the run log.
  Unknown or unsupported declarative options report `E0224` during elaboration.
  `per_unit_instance [= unit_type]` collects separate groups for the
  nearest containing unit of that type, including unsampled units.
  Group/basic-item `when` expressions gate sampling in their owning
  struct context. `instance_no_collect` uses `inst` to suppress a unit's
  group or item in reports. Hidden items remain available to crosses and
  transitions; filtered-out sources do not reuse stale cross values.
  Integral range widths subdivide inclusive bounds and preserve short
  final buckets. Hit goals resolve range → item → group → global;
  zero-goal buckets retain counts without contributing to bucket grades.
  `set_config(cover, at_least, N)` configures the default before sampling.
  Explicit range construction is capped at 65536 buckets per item.
  Basic-item `ignore`/`illegal` expressions use their item and constants;
  `instance_ignore`/`instance_illegal` additionally allow `inst` fields in
  per-unit groups (14.3/14.3.4). Ignored/illegal values add neither hits nor
  derived samples; wholly excluded buckets leave grade denominators and
  hole lists. Illegal values produce DUT errors after check hooks, or at
  observation when `check_illegal_immediately` is TRUE (29.9). The per-unit
  fixture verifies that ignore wins overlapping matches and deferred
  errors appear after `check()` and before `finalize()`.
  Integral/Boolean bucket masks classify comparisons/membership/Boolean
  compositions symbolically, or enumerate at most 65536 candidate visits
  per item refresh for other pure expressions. Wider arithmetic reports
  E0402. Real/string unbucketed items only filter observed values.
  Cross/transition `when`, `no_collect`, `instance_no_collect`, `at_least`
  and `weight` options run over basic or derived sources (14.4/14.5). Transition
  history advances through legal source samples even when the derived item
  is filtered. Goals inherit item → group → runtime precedence. Zero goals
  are ungraded; zero weights contribute nothing. Hidden derived items keep
  collection data but leave report/grade views. Derived grades participate
  in the weighted per-type aggregate of mixed per-instance groups.
  Named crosses/transitions may source earlier basic or derived items in
  the same group. Stable observation references preserve nested histories,
  duplicate bucket labels, masks and per-unit collections. Parent filters
  cannot leak stale values into child crosses. Source order, names and
  cardinality are validated with E0224, including across group extensions.
  Expanded tuples are capped at 65536 scalar components. Unbucketed source
  histories retain real/string labels but remain ungraded. Cross/transition
  ignore/illegal predicates bind participating basic scalar values; transition
  predicates also bind prev_<source> snapshots. Instance variants bind inst.
  Integral/Boolean masks use conservative interval proofs and iterative range
  splitting to find legal tuple witnesses, considering basic filters and
  removing wholly excluded combinations. Supported proofs include comparisons,
  Boolean logic, membership and checked arithmetic bounds. Unknown shapes
  retain ordinary evaluation. The budget is 65536 scalar-domain work units
  per derived item refresh, charging proofs and concrete checks; excess work
  reports E0402. Storage-boundary wrapping and short-circuit errors are
  preserved. Basic metadata lookup selects item declarations even when an
  extension's transition physically precedes its aliased source. Unbucketed real/string/enumeral values only filter actual observations.
  Masks commit atomically across basic and derived items and propagate to
  descendants. Composite predicate operands remain unsupported (E0224), and
  source-dependent classification over mixed composite inputs reports E0402.
  Whole-node constant predicates support nested sources. Full option type
  checking remains unfinished.
  Derived name options and range labels evaluate literal string expressions
  (concatenation/grouping) with runtime escape decoding. Full option values are
  consumed; invalid derived names report E0224 rather than accepting a prefix
  or silently using a default. Computed names share graph lookup and duplicate
  checks. Initialized plain integer/string const fields resolve in names,
  range bounds/widths/labels/goals and group/item goals/weights, including
  inherited and qualified static consts. Their supported pure initializers
  fold to literals for runtime storage too. Arithmetic reuses direct-operand
  precision contexts; cycles, unsupported dependencies and overflowing
  untyped arithmetic remain diagnostic cases. Aliases resolve through their scalar representation with a bounded walk;
  known int/uint widths 1–127 and star widths participate when initializers
  already fit. The runtime uses the same direct-operand representation/width
  resolution, including qualified static fields. General storage narrowing,
  unresolved widths, enums in constant expressions, dynamic initialization, derived real range masks and
  item-domain width proofs
  remain unfinished.

## What runs, by 2019 feature

| Feature | Clause | Notes |
| --- | --- | --- |
| Standard `/* */` comments, segment-spanning statements, brace groups | 4.2.3, 4.2.4, 4.3.2–4.3.3 | non-nesting comments, statements across code segments |
| `longint` / `longuint`, the six-way precision rules | 5.2.3.1, 5.6.3 | 32/64/unbounded/real modes truncate where operand types pin them |
| `list_ptr==` / `!list_ptr=`, list literals, `in_table`, `evaluate`, `typeof` | 4.6, 4.11.4, 4.11.8, 4.17.7, 5.9 | lists carry reference identity |
| Subtype qualifiers in dot access (`p.red'good'x`) | 4.17.3 | resolved against the synthesized when views |
| Field initial values | 6.9.1–6.9.4 | `const`/`!`/`~` prefix grammar |
| Static fields, methods, events; `Type::member` | 6.9.3, 17.2, 11.1.4.1, 11.2 | per-type store; the 17.2.2 restrictions checked |
| Interfaces: `interface`, `implementing`, casts, dynamic dispatch | 6.13 | E0414 for a missing implementation |
| Templates rework: value parameters, `conditions`, `template extend`, derived defaults, template interfaces/numeric types | 8.2–8.7 | canonical instance names computed after binding |
| Range-generated `~` fields, `rgf=`, the rgf routines | 6.9.7, 10.3.13, 18.6, 29.13 | reduced-set generation, deterministic concrete value |
| `keep target chosen_from read_only(list)` | 10.5.8 | seeded uniform pick among satisfying elements |
| Custom numeric types + `numeric_i` dispatch | 5.2.15, clause 31 | values by value; missing `numeric_*` method is E0418 |
| Simple macros | 15 | expand before parsing |
| `repeat until`, the `do` sugar | 20 | |
| Tables: `table … with { … }`, `csv_to_table`, the `table_manager` API | 26, 32.3 | token pre-pass between `#define` and macro expansion |
| Annotations: `annotation @name`, attachments | 35 | recorded metadata, no semantic effect |
| `semaphore` / `rdv_semaphore` / `locker`, the resource-sharing TCMs | 33 | blocking `up()`/`down()`/`lock()` park on a FIFO wake pass; `try_up()`/`try_down()` never switch; `set_value`/`set_max_value` rules enforced |
| Reflection catalogue + `instance_iterator` | 32.1–32.2 | catalogue in [REFLECTION_2019.md](REFLECTION_2019.md); the base iterator walks the instance tree |
| Set/list pseudo-method subsets, `set_of_values` | 5.10.5–6, 28.5, 27 | `set_of_values` reflects constant integer subtype bounds, including through aliases. `full_set_of_values` uses the representation bounds. List numeric aggregates, keyed extrema, filters and Boolean reductions support per-item expressions |
| Simulator side | 23 | `rise/fall/change(HDL expression) @sim` events and `hdl_path()`-bound event ports sample each virtual cycle (a live xezim session, or an injected history); TCMs declared `@event` commence on the event |

Recognised-but-unimplemented 2019 library names (the full list sits in
`UNIMPLEMENTED_2019` in `src/interp.rs`) parse, arity-check, and report
**E0419** with their clause at run time instead of failing as unknown
names.

## Differential qualification infrastructure

`scripts/qualify.py` checks a self-contained manifest corpus against independently
expected values. It supports native-only checks and a configured reference
adapter with strict case/source/seed identity and success envelopes. Reports
retain owned result data and fingerprints, not vendor output dumps. The initial
three candidates pass natively and CI verifies the runner protocol. No licensed
Specman adapter or actual reference comparison of these candidates is qualified
yet; native-only reports explicitly record reference-not-run. See
[qualification/README.md](../qualification/README.md).

## Native HDL profile

The [explicit xezim 0.11.3 profile](HDL_QUALIFICATION.md) runs real VPI
simulation, checking signed 32-bit transfers, both mixed counter examples,
error diagnostics, VCD transitions and HDL code coverage. The native session
has bounded startup/command/finish handshakes and reaps its owned child on
failure. Wider/MVL transfers are rejected; E1647_XEZIM_WAVE enables optional
wave instrumentation. This qualifies explicit hdl_run/READY observations,
not general e/HDL event ordering. Default cargo tests cover transport and
frontend mechanics. A separate CI job builds the current Dockerfile and runs
the real profile, retaining owned evidence and image identity. The Dockerfile
image and both qualify-hdl/verify Compose services have also passed locally,
including 513 all-feature tests; see the profile's recorded build identity.

## Tests and the two revisions

The suite targets **2019 only** — there is no 2016 mode and no
per-revision test split. Coverage comes from two layers: the
pre-migration tests (authored against 2016-era behavior) pin the
**shared subset**, valid because the 2019 revision is almost purely
additive; the `tests/fixtures/s2019_*` fixtures and the migration's
tests pin the **2019-only additions**. The 2016-era corpus files under
the verification log's regression set (etl, cover-edges, the golden
testbenches) act as the legacy-behavior check — they were re-run on
every migration merge. Where a rule genuinely changed between the
revisions (the migration plan's §1.2 items: `extend global`'s wording,
bool in `set_of_values`, the numeric list aggregates, set iteration,
`deep_copy`'s generality, the phase naming, the 64-bit precision
rules), the pinning tests carry a "differential reading" comment
saying what 2016 did differently; the tool implements the 2019
reading, nothing else.

## What does not work yet

- A full type system (the [`typing`] conservative pass reports definite
  mismatches across method bodies, field init values, `keep`
  constraints, `on` bodies, coverage items and the synthesized `when`
  views; local declarations propagate known initializer kinds, but
  ambiguous kinds stay unknown and there is no interprocedural type
  inference or full name-resolution pass. Supported coverage option
  syntax and unit scopes are checked. Boolean filters/visibility and
  ignore/illegal predicates report definite type mismatches with owner,
  sampled-source and selected-unit bindings. Define kinds, composite
  operands and full option type checking remain unfinished).
- General temporal sampling remains incomplete. Temporal
  `change/rise/fall` now retain Boolean/integer/enumeral samples (numeric
  rise/fall compare magnitude; the first value establishes a baseline).
  Explicit event clocks gate edge sampling; active waits retain edges
  arriving after their turn. Propositions and repeat bounds retain their
  sampled values: rules sample before each global cycle closes and at quit;
  waits sample when checked, with a separate context for each suspended
  wait. Rescans use those recorded values, including through mutable
  list references. This is expression-result history, not an arbitrary
  snapshot of every variable or a general event-specific timeline. Physical
  event/rule ownership is per instance, but all use one global cycle log. Real/MVL/custom-numeric edges and static edge-width checks
  remain unsupported. Multiple occurrences in one virtual cycle collapse
  to the cycle's sample.
  Unbounded obligation windows exhaust when the run ends — that is the
  12.3.3 semantics, not a limitation.
- Constraint solving beyond bounded shapes (`gen` handles the linear
  and bounded-enumerable non-linear subsets — polynomial products of
  total degree ≤ 4 over capped domain products; wide domains, division
  and modulo still defer to rejection sampling; `chosen_from` and the
  rgf reduced sets are the sampling-side exceptions).
- General testbench execution (sequences run inline; the e↔HDL event
  ordering is not implemented; the VHDL backend is unwritten).
- The remaining out-of-scope list of the 2019 migration plan (TLM
  sockets, Excel readers, coverage-database files, message-manager
  tables, encryption, the full reflection model) — semaphores moved out
  of it when the clause 33 structs landed.

**No IEEE conformance is claimed.** Where this implementation's reading
of a clause is an approximation, the approximation is documented in the
nearest code comment and the PR that introduced it.
