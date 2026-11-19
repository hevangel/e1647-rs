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
single-source browser sandbox API runs in disposable workers (see
[BROWSER_SANDBOX.md](BROWSER_SANDBOX.md)).

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
  in the first column with no other text on that line (clause 4.2.3);
  prose outside those segments is skipped. A file with no begin marker
  is tokenized as code throughout. Decimal integers take a `K`/`M`
  multiplier, `0c"..."` is a character literal, and non-decimal based
  literals accept MVL digits. An adjacent identifier/tick suffix takes
  qualification precedence, including flag and fully numeric-looking suffixes
  such as hFF. Otherwise an unsized numeric candidate must end at an identifier
  boundary; separate complete radix spellings stay numbers. A closed simple
  quoted path preserves its quotes even when its name looks numeric. The
  cursor remembers the previous identifier's byte end; whitespace/comments
  break adjacency, preserving separated numeric macro arguments. Escaped or
  indexed path parsing is not added. Reports the first lexical
  error; later stages collect many. MVL literals are tokenized and not
  evaluated. Block comments (`/* */`) are standard (clause 4.2.4):
  they span lines and code segments and do not nest.
- `src/ast.rs` — typed AST for declarations (including `type` enumerals,
  `when` subtypes, `simple_port`, `event`, `import` and `define`), fields,
  type expressions, constraint expressions, methods (including a TCM
  `@event` suffix) and statements, all carrying byte/line/column spans.
- `src/parser.rs` — recursive-descent parser for the declared subset.
  Expression precedence follows clause 4.6 (Table 12): C order for the
  arithmetic and boolean operators, with `^` looser than `|`, `===`
  looser than `==`, the `~` / `!~` string-match operators between `===`
  and `in`, and `is [not] a <type>` between `==` and the comparisons. A
  bare `is` that does not start a type check is left for the caller. A
  user-written `extend global` is rejected (`E0116`, clause 4.4.2.2);
  the parser's file-level `define`/`import` pseudo-declaration is
  unaffected. Error recovery is at declaration,
  member and statement boundaries. The statement subset includes braced `if`/`while`,
  `for each` and `for i from .. to`, `case`, `check that`, `wait`, `emit`,
  `start`, sequence `do` and `stop_run`; `repeat [do] { .. } until
  cond;` (20.2.3, the body at least once) and the optional `do` sugar
  on `if`/`while` — including the one-statement `if cond do
  <statement>;` form — round out clause 20. Struct members include `on`
  event blocks (clause 11.2: dotted event path, action block; extension
  is a repeated declaration) and `on <port>$` event-port blocks
  (clause 11.3: the 9.5.2 `$` access operator names the port's event;
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
  parsing (21.3), and `#else` flips the included side — both
  `} #else { .. };` and `}; #else { .. };` spellings (the 21.3 syntax
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
  `template extend` (8.6) is flagged `template_extend` with its own
  parameter list instead of being folded by name; formals
  (`<type>`), template references (`deque of int`) and keyed lists
  (`list (key:it) of t`) are TypeExpr variants. Template instances
  monomorphize (8.7): a worklist
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
  the extension's parameter specifications (8.7's NOTE 3): the
  prose-named `scalar`/`enumerated` categories check the actual's
  shape (8.2.2's category table is an image, so any other spelling is
  read as 8.6's specific-type form and can only ever mismatch), the
  original template's own categories stay unvalidated (8.7's NOTE 2),
  and an arity mismatch reports `E0304` once and skips the extension.
  Typed `new <named-or-template-instance> [with { .. }]` instantiates
  and runs the `with` assignments (clause 6.3); untyped `new` uses a known
  contextual instance type or reports E0402 when no such context resolves.
  Clause 14 coverage groups (`cover <event> is [also] { .. };`, `is
  empty;`) record their item/cross/transition entries structurally and
  every option clause verbatim; the derived entries synthesize
  `CoverDerivedData` — `transition__item` collects the base item's
  previous→current bucket-label pairs (14.5: n samples → n-1
  transitions) and `cross_a__b` records every combination of the
  crossed items' labels (14.4), each graded over its sources' bucket
  cross-product when all sources have buckets.
  Clause 11.4 `expect`/`assume` members parse their temporal
  expressions into the clause 12 grammar (`TemporalExpr`: event atoms,
  `cycle`, propositions and edge samples, `{;}` sequences, `=>` yield,
  `and`/`or`/`not`/`fail`/`eventually`, repeats, and the binary
  sampling `te @event`; `detach`/`delay`/`exec` report `E0120`) with
  the `else` failure body parsed as actions. Clause 10
  `select { weight : choice; .. }` distributions parse — weights
  structurally, choices verbatim — and the rejection sampler applies
  them (10.3.12.3): each `field == select { .. }` leaf draws an arm
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
  library driven by the interpreter: `sampled_normal_form` propagates
  a temporal expression's sampling event so every atom carries it
  (12.1.4; sub-sampled subtrees keep their own event, 12.2.6) and
  `holds_tightly`/`succeeds` evaluate tight satisfaction (12.1.1.2) and
  last-point success (12.1.1.12) over explicit finite paths of
  `State`s — each state records which events occurred. Operators follow
  their subclauses: sequences over strictly consecutive segments
  (12.2.9), yield as a universal check over the lhs's successful
  prefixes with vacuous hold when it fails (12.2.14), fixed and
  true-match repeats (12.2.10–12.2.11), first-match repeats as sequence
  items (12.2.12), `fail`/`not`/`eventually` (12.2.13/17/15).
  Propositions and repeat bounds are evaluated through caller-supplied
  callbacks or recorded history; `change/rise/fall(exp)` read recorded
  edge results (12.2.18). Missing or unsupported edge samples report
  `EvalError::EdgeSample`
  and unresolvable constructs report `EvalError::Unsupported`. The
  same module hosts the run-loop monitoring helpers:
  `succeeded_somewhere` (12.3.2 at any point), `rule_failed` (the
  11.4/12.3.3 expect/assume failure reading over a finished run log —
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
- `src/typing.rs` — conservative post-composition type checking over
  methods, field initial values, constraints, `on` bodies and coverage
  items (14.3), including synthesized when views. Coverage expressions
  resolve against the owner; items without expressions use the owner's
  same-named field. An explicit item type checks compatibility (E0220),
  and expression traversal checks missing members, indexing and operand
  kinds (E0221–E0223). Unknown kinds stay silent. Cross/transition names
  refer to items rather than owner fields. Effective Boolean options are
  checked using the shared coverage option parser: `when` uses the owner,
  instance visibility also binds the selected unit as `inst`, and
  ignore/illegal predicates bind sampled basic sources without an owner
  receiver, including `prev_<source>` for transitions. Instance effects
  also bind `inst`; it takes precedence over a same-named source. Each
  expression must be compatible with bool (E0220); ordinary conservative
  traversal also reports E0221-E0223. Unknown kinds, define constants and
  composite-valued sources remain conservative. Group extensions use the
  final effective options. Scopes are discarded before checking methods
  and other types. This does not add complete name resolution or type
  checking for every option/expression form. Supported option syntax and
  unit scopes are checked during elaboration. Typed method parameters bind within their method scope.
  Local `var x := e` declarations retain known initializer kinds,
  including homogeneous list literals and equal-kind ternary branches
  (clause 18). Scalar/list aliases resolve with a depth bound; recursive
  aliases, custom numeric dispatch, mixed/empty list elements and
  ambiguous alternatives stay unknown. Arithmetic on unknown operands
  also remains unknown. This is declaration inference, with no
  flow-sensitive narrowing or interprocedural inference; full name
  resolution remains a gap.
  Expression traversal is exhaustive over the AST's ordinary value children:
  membership elements and sets, casts, bit slices, table tuples/rows, set
  literals, select weights, chosen-from targets/lists and unknown callees.
  Retained type expressions now traverse scalar/formal widths, nested list
  elements/keys, template arguments and unresolved typeof placeholders.
  Keys bind it to the list element kind; all parameters are bound before
  checking dependent method annotations. Definite non-integer widths report
  E0223. The visitor clears value hints, restores local kinds and discards
  type-position binding effects afterward. It visits alias/field/method/local,
  coverage and expression type sites, with a recursion bound of 64. Expressions
  already replaced by elaboration cannot be recovered by this pass; raw select
  choice tokens still need their grammar. Cast result kinds stay unknown because the
  evaluator's fallback preserves its input. Explicit `evaluate` results,
  slices and `in_table` results have known kinds. An evaluate body gets a
  scoped result local and keeps the owner/caller context; a new initializer
  gets a fresh locals frame, no enclosing return type, and the known new
  object as its receiver. Scoped expected kinds now reach evaluate/new
  from known variables, assignment targets, field initializers, method
  parameters and returns. Explicit expression annotations take precedence.
  Calls clear the enclosing hint and bind each argument's parameter hint;
  list predicates supply Bool. Expression bodies clear the enclosing hint
  before walking actions; their own declarations/assignments supply hints.
  Constructor elements can use a surrounding list's element context, and
  concatenation type inference flattens list operands to match evaluation.
  Materialized templates retain object kinds. Unknown contexts remain
  conservative; this is not general bidirectional or flow-sensitive inference.
  All statement value children now traverse, including loop bounds, print,
  gen, emit, rgf assignment, wait, start and compute. The recorded foreach
  index-name path is structural, not treated as an evaluated expression.
  Temporal rule/wait propositions, edge operands and repeat bounds traverse
  recursively, as do rule else actions; event reference paths remain
  structural. A cloned exact-count repeat bound is checked once. This adds
  no temporal execution support or new temporal operand typing rule.
  Events/ports have unknown value kinds rather than false missing-member
  errors. Known method arguments are inferred once, then validated against
  their signatures without re-traversing them. Known list indexes require
  integer/enum kinds (E0223); their integer context is independent of the
  result's context. Membership unwraps a contained list's item kind before
  comparison, checks known list/set containers and rejects definite element
  mismatches (4.11.6). Declared sets, literals and set_of_values have a Set kind;
  scalar domains and unsupported numeric conversions remain conservative.
  Operand traversal runs once without inheriting the Boolean result context.
  Explicit is-a bindings retain facts for true and false Boolean outcomes.
  Lazy and/or/implication inference checks operands once under their entry
  facts; joins retain identical kinds only. Possible binding writes invalidate
  earlier facts, including shadowed frame locals. Branches, while bodies,
  check failure actions and ternary arms install temporary proof scopes.
  This does not narrow the tested operand or change interpreter binding
  lifetimes. Define and predefined constant precedence now
  applies across every checked context, preventing same-named fields or
  locals from supplying the wrong kind; define values remain unknown.
  Registered `Type::field` accesses read kinds from the exact declaring
  registry; qualified calls check arguments once against that owner's
  prototype and retain the declared result kind. E0307 still owns invalid
  static-member shape/ownership checks. Static entries are checked once
  from their registries, skipping inherited/synthesized copies. A static
  method/handler has its declaring namespace but no `me`; bare static fields
  follow the bounded base-chain lookup after locals. Static initializers
  have neither receiver nor a bare static namespace, matching current runtime
  initialization. A declaration is preferred over physically earlier
  extension blocks; omitted extension annotations inherit by parameter
  position/result. Explicit extension annotations, arity and static prefixes
  now receive E0225 contract checks after composition/template substitution;
  general type equivalence stays conservative. Unregistered/template entries stay
  conservative. New initializers clear the caller's static context and restore
  it afterward. Bare static method calls remain unsupported in the runtime
  and unknown to this checker; qualified calls are the supported form.
  The runtime carries contextual TypeExpr hints separately from its existing
  arithmetic-precision lookup. Context lookup inspects metadata without
  evaluating receivers or indexes: it follows known members, static fields,
  list elements, declared method results, equal-shaped branches/literals and
  supported inferred-value shapes. Alias lookup is bounded; inferred list
  shape recovery is depth-capped and memoizes identities to reject cycles and
  avoid repeated DAG traversal. Call argument lookup runs only for expressions
  that can use a hint. Field initializers receive their declaration type.
  One method-frame builder binds prototype metadata and return types for
  inline/static/statement calls and started TCMs, preserving it in suspension
  snapshots. New/init/evaluate bodies isolate caller hints; failure paths
  restore expected types. E0122/E0402 remain the runtime diagnostics when a
  required evaluate/new context is unavailable. No numeric-width inference,
  full subtype inference or type-embedded resolution is added here.
  Native membership compares list items through ordinary equality, with
  input snapshots and distinct candidate matches for list-to-list containment.
  Iterative augmenting paths avoid a greedy mismatch when mixed numeric
  equality gives overlapping candidates; no recursive search is used. Set
  membership checks each scalar or left list item against canonical intervals,
  converting Boolean/enumeral values to their indices. Set multiplicity is
  irrelevant. Real-valued set membership and full domain qualification remain
  gaps. Inferred native set values also supply a contextual set hint.
- `src/elab.rs` — member composition: `like` chains and `extend` blocks fold
  into one composed member list per `struct`/`unit` (inheritance-cycle,
  cross-declaration duplicate-field and unmodified-method-redefinition
  diagnostics; codes `E03xx`). Also records `simple_port` HDL paths,
  `on` members (base types first, extends in declaration order),
  recovered `sequence` declarations, and a `type` declaration's
  underlying representation. `when` subtypes (6.7) resolve: each
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
  subtypes; the separate conservative type pass checks composed views.
- `src/builtins.rs` — predeclared names the executed subset relies on:
  the `sys` unit, sequence bases (`any_sequence_item` and siblings), and
  routines such as `out`, `outf`, `message`, `pack`, `append`,
  `dut_error`, `stop_run`, the objection no-ops, and the `hdl_*` bridge
  calls; the predeclared type names `file`, `event` and `set` (5.2.12).
- `src/coverage.rs` — the clause 14 coverage data model, built from
  the recorded cover groups at run start: `is also`/`empty`
  extensions merge into their base group by name (14.6, either
  declaration order), `using ranges = { range([lo..hi], "label");
  ... }` clauses parse into explicit buckets, and items without them
  take the 14.3 defaults (16 buckets for integers — "0 through 15" —
  two for booleans; anything else is ungradeable with a note). The
  interpreter samples a group when its event fires: each item's
  recorded expression evaluates in its owning struct (or the named
  field is read) and the value lands in the containing bucket;
  out-of-range values are ungradeable (14.3: "goals of 0"). Grades
  are covered-buckets-per-item, averaged over gradeable items; cross/
  transition items grade over their sources' bucket cross-product.
  The option tokens apply where the prose names them: `at_least = N`
  is the per-bucket goal, `per_instance = TRUE` (item or group
  spelling) makes each bucket its own graded instance (14.3.2 —
  `CoverInstanceData`, name `item==bucket`, holes for unsampled
  buckets, the group grade = the weighted instance grades plus the
  per_type bucket), `weight = N` scales the grade contribution, the
  group-level `at_least`/`weight` spellings apply to items without
  their own (14.9.3/14.9.4), `no_collect`/`instance_no_collect`
  (group, 14.2.2; item, 14.3.4) suppress presentation while retaining
  sampling storage, and the cross/transition `name` option
  renames the derived item (14.4/14.5). Derived `when` filters run with fresh
  owner frames: transitions at the source observation, crosses after basic
  sources for the round. Filtering a transition does not rewind the last
  legal source observation. Derived visibility options keep collected data
  but remove presentation and grade contributions, including per-unit
  visibility changes. Goals count combinations reaching their required
  hits, with item > group > runtime defaults; zero goals are ungraded.
  Weights scale derived contributions within the same per-type aggregate
  used by ordinary and mixed per-instance groups. Zero weights contribute
  nothing, and floating-point weight totals avoid u64 sum overflow.
  Checked u64 products keep large combination domains ungradeable rather
  than overflowing; transition totals avoid host-pointer-width arithmetic.
  Derived nodes can reference earlier basic/derived names in their group.
  Observation keys reference stable basic bucket indices or parent key
  indices, independently of display labels. Each source retains its previous
  observation and current-round freshness. Transition descendants run from
  a declaration-ordered work set as their source is produced; crosses run
  in declaration order after basic sampling and produce further transitions.
  Filtering a parent produces no child source; filtering a transition still
  advances its parent history. Hidden nodes remain sampling sources.
  A linear pass recomputes nested validity and combination counts against
  current basic masks, preserving original keys for mask restoration. Empty
  source domains dominate unknown/unrepresentable products. Unbucketed
  strings/reals retain value labels while grades remain unavailable.
  E0224 validates source arity, unique names and earlier declarations in the
  merged base-then-extension order. A 65536 expanded scalar-component cap
  bounds tuple expansion separately from bucket cardinality. Cycles are
  rejected as invalid forward/self references; iterative processing avoids
  recursive source-graph walks. The library round API requires begin_sample,
  sample for each basic source, then record_crosses to finalize derived stats;
  completing the same round twice does not add duplicate derived samples.
  Scalar basic observations additionally retain owned value snapshots, separate
  from labels/identities. Cross predicates bind their participating basic items;
  transition predicates bind the source and prev_<source>. Instance variants
  also bind the unit as inst. Pure-expression validation rejects owner/local
  access, calls and direct composite-valued operands (E0224). Runtime Boolean
  checks are E0402. Ignore short circuits illegal, using the existing overlap
  policy and immediate/deferred DUT reporting. Suppressed derived observations
  do not become child sources; basic source history still advances.
  Derived integral/Boolean classification first proves uniform outcomes over
  the remaining source envelope. `coverage/proof.rs` propagates conservative
  integer intervals and total Boolean domains, respecting short-circuit logic,
  conditionals and membership evaluation order. Arithmetic bounds require
  checked endpoints; possible storage overflow stays unknown. Singleton inputs
  use the real evaluator. Constant/name lookup respects define/predefined
  precedence and the implicit inst binding. The current predicate-frame numeric
  semantics are preserved; this adds no field-width precision support.
  Uniform results retain the full source domain or exclude the node without
  Cartesian enumeration. Otherwise bucket tuples use an iterative box work
  list, exact proofs and concrete lower-end witnesses. Unknown boxes split the
  first referenced varying axis; a sign-biased u128 midpoint handles the full
  i128 domain without overflow. Basic ignore/illegal predicates participate in
  every proof/check. A shared 65536 scalar-domain work budget charges each
  interval proof and concrete check by its axis count. Excess work diagnoses
  E0402 instead of inventing a mask. No recursive box search is used.
  Basic source declaration/prototype lookup is shared with the basic classifier
  and filters by Item kind, preventing earlier extension transitions from
  shadowing aliased source types. Integer-minimum negation reports E0402 rather
  than panicking. Constant prefixes can still exclude nodes before domain work.
  All basic and derived masks commit together after successful classification.
  Unbucketed real/string/enumeral sources filter points only and remain ungraded.
  Constant predicates work over nested sources; source-dependent classification
  over mixed composite inputs remains E0402. Composite-valued predicates,
  general wide-domain classification, typed-domain/precision proofs and exact
  side-effectful filter ordering still need implementation/qualification. The 14.9 coverage
  configuration (`CoverageConfig`: the coverage mode per 14.2's NOTE
  and `max_int_buckets` per 14.3's bracket) is honored through the
  `build_with_config` builder API and `set_config(cover, ...)` (29.9).
  The interpreter builds the schema independently of collection mode,
  which starts disabled. It validates category/option keyword positions
  before evaluating values and commits option changes together. Mode
  changes gate emit-driven and procedural sampling without clearing data.
  Before sampling, `max_int_buckets`/`at_least` changes rebuild the schema from the
  composed type declarations; after sampling they report E0402. The
  runtime resource bound is 65536 buckets per default integer item.
  Per-unit groups keep a schema plus one collection per containing unit,
  labelled by `CoverGroupData::unit_instance`. The bounded, visited-set
  instance walk resolves the nearest requested unit through inheritance,
  flattened lists and cycles; aliases use the first visited path.
  Unsampled units keep zero-hit groups. Structural option/scope errors
  report E0224 after composition. `when` expressions use a fresh owner
  frame; per-unit presentation expressions bind `inst` to the associated
  unit. Crosses require current-round source values, and transitions keep
  previous sampled values separately. The returned report view removes
  hidden items/groups and per-unit schema placeholders. Full option type
  checking, derived-item ignore/illegal filters and radix remain unfinished.
  Basic-item ignore/illegal predicates run in fresh frames with only their
  item, constants, and (for instance variants) the associated `inst`. Calls
  and owner/local access are rejected with E0224. Non-Boolean results and
  unsupported wide classification report E0402. Comparisons, membership
  and logical compositions classify integral/Boolean intervals exactly;
  other pure shapes enumerate with a shared 65536-candidate budget per item
  refresh. Short circuit domains prevent evaluating guarded arithmetic on
  excluded values. Masks commit atomically per group and refresh at sampling,
  grade computation and final reporting. Partial buckets remain gradeable;
  fully ignored/illegal buckets leave the denominator and report view.
  Legal observations retain their original counts; ignored/illegal observations
  are never recorded. Derived keys retain bucket indices separately from
  display names, enabling current masks to filter historical combinations
  and recompute possible-combination totals. Restoring masks reveals earlier
  legal history. Unbucketed real/string items filter actual observations only.
  Illegal observations queue DUT errors until after check hooks, with a final
  flush after finalize/stop; check_illegal_immediately reports at observation.
  Ignore takes precedence over illegal on overlap. This precedence, mutable
  masks and exact reporting-phase placement are not reference-qualified.
  Predicate arithmetic uses the interpreter's current scalar model; complete
  typed-domain and precision proofs remain future work.
  Integral `range(...)` construction parses positional name/width/goal
  parameters, subdivides inclusive bounds and uses stable automatic bucket
  labels. Range-local goal metadata preserves precedence over item/group/
  global defaults. Zero-goal buckets retain counts but leave the bucket-grade
  denominator; they do not report holes. Checked i128 constant arithmetic
  rejects overflow/division errors, and construction is capped at 65536
  buckets per item. General constant-name/type resolution, real/enumeral
  bounds and item-domain width proofs remain unfinished.
- `src/interp.rs` — interpreter for the statement subset (codes
  `E0401`–`E0413`). List values (clause 4.11.4's `list_ptr==`) carry
  reference identity: a `Value::List` holds its items behind a shared
  `Rc<RefCell<Vec<Value>>>` handle — assignment aliases the same list
  object, `copy()` (27.4.3) mints a fresh one, the 27.3 modifying
  pseudo-methods mutate the shared storage in place, and the recursive
  read paths are depth-bounded. Entry: the built-in `sys` unit's `run()`
  methods, with
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
  and `hdl_path()`-bound event ports (9.5, 9.7.3) are edge-sampled
  every virtual cycle while an HDL source is loaded — a live session or
  the injected signal-history test seam (`hdl::SignalSource::Injected`;
  without one they never fire). The clause
  13.2.2 scheduler runs concurrent TCMs: the `run()` bodies are the
  main coroutine and every `start`ed TCM joins the queue with the
  implicit sync cycle before it commences (a TCM declared `@event` with
  a declared sampling event commences when that event occurs — 17.1.2,
  17.2.1); coroutines take round-robin
  turns that run atomically from one synchronization action to the
  next, and when nothing is eligible one cycle passes (the 5,000-cycle
  shared budget guards runaway programs, `E0409`). Suspending
  captures locals and the full statement path through compounds and
  statement-position calls. Each call boundary retains the receiver,
  evaluated arguments, callee locals and composed-body index. Nested
  helpers and `compute` suspend on the same coroutine; returning
  restores the caller without replaying its prefix. Entry-event waits
  park through the scheduler's event wake pass. Compute argument evaluation
  uses the same pure parameter-context lookup as ordinary/started calls.
  Parameter/return metadata survives nested suspension and composed-body
  transitions. Instance event keys encode physical owner plus event/port
  name. Static event keys retain Type::event; bare/dotted/me/self/alias paths
  bind in the owning frame. Emission receiver expressions evaluate once and
  on/coverage processing occurs once for the physical owner. TCM entry paths
  bind to the callee and already evaluated parameter values, not its caller.
  Temporal expressions bind dotted event paths at rule registration or wait
  entry, retaining those identities through suspension; the pure core still
  accepts opaque string event keys. Static temporal paths use one
  type-qualified segment. Indexed temporal path syntax and active event
  reference rebinding remain unsupported. Scheduled value calls attach a
  `StatementEvaluation` tree to the statement path and descend through a
  synthetic call level. Completed evaluation nodes retain values and ordered
  child positions; replay traverses cached nodes until the pending call,
  resumes its saved frame, then continues the original expression. Keeping
  the original AST preserves operand precision and contextual type lookup.
  Operator results and formatted print parts are evaluation nodes too, so
  completed custom numeric callbacks do not replay. Each committed while
  iteration resets its header tree; repeat-until records whether the body
  completed before a suspended condition. Composed calls retain their last
  return value. A private `ExecError` separates suspension/stop control from
  public diagnostics; statement boundaries translate control into `Flow`.
  Scoped evaluate/constructor actions, list callbacks, generation/coverage
  observation, static methods, `on` blocks and check failure bodies remain
  atomic. Any additional legal suspension in scoped callbacks requires
  separate handling for their locals and partially committed effects.
  Static methods are not TCMs (17.2.2); on actions prohibit time-consuming
  actions (11.3), and evaluate actions exclude wait/sync/return (4.17.7).
  Remaining callback work must preserve those restrictions. Temporal waits
  (`@event`, sequences, propositions, event
  repeats, with 13.2.4's readability-only `until`) re-evaluate at
  every turn (12.3.2); propositions and repeat bounds retain sampled
  results in each state, scoped to the individual wait. Rules retain
  their own samples before global cycles close (including inline time
  advances) and at quit. Runtime rules are materialized once per physical
  created instance, including later allocations, with independent monitor,
  sample context and registration-cycle log window. Uninstantiated types create no
  monitors. Failure actions execute on the declaring instance. A wait's
  age survives repeated checks, while the next span/start gets a fresh budget. The pure core reads these historical values,
  and numeric repeats read their bound at the path's starting state;
  callbacks remain available for external caller-supplied paths. Missing
  history does not fall back to present-day values. This is expression
  result history, not arbitrary variable snapshots or event-specific
  sampling timelines. Scalar edges retain their previous sampled value
  per evaluation context and expression. Boolean/integer/enumeral values
  are supported; numeric rise/fall compare magnitude, and the first
  sample establishes a baseline. Explicit event clocks gate sampling;
  suspended waits capture those clocks at cycle closure and consume
  matches since their previous check, so a clock after the waiter's turn
  is retained. No repeated whole-prefix scan is needed: only the prior
  in-progress state and newly appended states can gain matches. Finished
  or expired wait contexts retire their sampling state. Real/MVL/custom
  numeric edges and static edge-width legality remain gaps, and multiple
  occurrences in one virtual cycle collapse to that cycle's sample.
  A temporal wait
  unsatisfied after 1,000 cycles reports `E0409`, and unevaluable TEs
  (unsupported edge values/clocks, unresolvable bounds) report `E0402` honestly.
  `sync` (13.2.3) parses in bare and TE forms — the bare form
  suspends one cycle, the TE form behaves like the temporal wait.
  `expect`/`assume` rules (elaborated per type, 11.4) stream through
  per-rule monitors at finalized cycle boundaries (12.3.3): invariant
  violations fire then, and obligation verdicts finalize when their
  windows close. A failed rule runs its `else` body
  on the sys instance as a check error, or prints the failure when
  there is no else body. One global timeline — no per-instance
  sampling. `emit` also samples the cover groups (clause 14) of the
  instance whose event fired, recording item values into the
  `src/coverage.rs` buckets; `run --coverage` prints the report.
  `gen <obj>` gates the object with the enclosing instance's
  constraints that reference it (10.3.6-10.3.7's connected field sets, v1):
  depth-two paths rebase (`p.v == select { .. }` → `v == select
  { .. }`) so when-view distribution policies drive generation;
  constraints that do not rebase (other roots, deeper paths,
  whole-object equalities) drop until full CFS scoping lands. The
  reflection set (5.10.5/28.5) evaluates: `set_of_values(type)` builds
  a `Value::Set` — canonical inclusive intervals (sorted, merged,
  non-neighboring) over the type's width/signedness or enumeral
  indices — and the set pseudo-methods (`size`/`uint_size`,
  `min`/`uint_min`/`int_min` and the `max`/`uint_max`/`int_max`
  mirrors, the `_is_uint` guards, `union`/`intersect`/`diff`) run on
  set values; `to_string()` renders scalars and sets, and `as_a` to
  an enumeral type maps the integer index to the enumerand. The clause
  27 list pseudo-methods dispatch on `Value::List` receivers: the 27.3
  modifiers (`push`/`push0`/`add`/`add0`/`pop`/`pop0`/`insert`/
  `delete`/`fast_delete`/`clear`, a list argument extends per the list
  forms 27.3.2/27.3.4/27.3.14/27.3.15), the no-argument general forms
  (`size`, `is_empty`, `exists`, `copy`, `reverse`, `top`/`top0`), the
  no-argument extremes (`min`/`max`/`min_index`/`max_index` — ties go
  to the latest item, empty is an error) and aggregates (`sum`,
  `product`, `and_all`, `or_all` — with their stated empty-list
  results), and `is_a_permutation`. Numeric expression aggregates run.
  `min`/`max`/`min_index`/`max_index` additionally accept one key
  expression bound to `it` (27.4.15/16/18/19). Numeric/string keys use
  relational ordering; the latest tied item wins. The result is the
  original item or its zero-based index, preserving object identity.
  Traversal snapshots the receiver before key evaluation. Empty lists,
  excess arguments and unorderable keys report E0402; expression errors
  propagate. The type pass scopes `it` to the element kind and infers
  the item/index result kind. Implicit index bindings and the other
  expression-taking pseudo-methods remain documented gaps (27.1 makes
  them macros); see the interpreter's module docs.
  `has`/`count`/`all`/`all_indices` also accept one boolean expression
  (27.4.10/4/29/30). These queries bind `it` in a temporary scope,
  snapshot the receiver before traversal and preserve order; `has`
  stops at the first match. `all` returns a fresh list with shared object
  elements, and `all_indices` returns matching zero-based indices.
  Empty lists return FALSE, zero or a fresh empty list. Arity and
  non-boolean predicate errors report E0402. The type pass scopes `it`
  to the element kind and infers the query result kind.
- `src/solve.rs` — constraint IR and the pluggable solver seam: the
  top-level `keep` constraints of one type translate (conservatively) into
  bounded-integer variables plus a boolean tree of comparisons;
  range membership (`x in [lo..hi]`) translates to the two comparisons,
  item-list membership becomes a disjunction of equalities. Disjunction
  and negation trees translate. Linear problems use Pumpkin behind the
  `solver-pumpkin` feature; polynomial problems of degree up to four use
  feature-independent enumeration with capped domain products. Both
  paths are deterministic per seed; unsupported problems fall back to
  rejection sampling.
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
  Native sessions use a bounded reader queue, 16-KiB lines and one deadline
  for a complete exchange (including RUN's OK/READY). Reserved response
  tokens match exactly; ERR/malformed/out-of-order replies close and reap
  the child. Checked finish observes BYE and process status; the CLI reports
  failure as E0408. Signal transfers are explicitly signed 32-bit with
  wider/MVL values rejected. E1647_XEZIM_WAVE controls optional --wave.
  The concrete profile and its separate artifact-verifying runner are in
  [HDL_QUALIFICATION.md](HDL_QUALIFICATION.md); they do not prove general
  e/HDL event ordering.
- `src/main.rs` — `e1647 tokens`, `e1647 parse`, `e1647 elab` and
  `e1647 run [--seed N]`. `import a.b` loads `a/b.e` or `a.b.e` next to
  the source and splices those declarations in front. `parse`, `elab` and
  `run` print diagnostics to stderr
  (`path:line:col: error[code]: message`) and their results on success.

Present concrete method prototypes now receive a separate E0311 interface
contract check in the well-formedness pass (6.3/6.13). Effective interface
requirements include transitive interface bases; prototype lookup follows
the struct's like chain and extend blocks. It reports definite arity,
annotation-presence, parameter/result shape/width and static mismatches, once
per type/method. Plain representation aliases expand with a 64-step bound;
constant expressions normalize widths and template value arguments. Bit,
nibble and byte normalize to their unsigned widths; long bases retain their
distinct arithmetic identity. Missing prototypes, symbolic/ranged types and
unresolved/conditional/template-only cases stay conservative; missing actual
implementations still report E0414 when called. General signature equivalence
remains a gap. E0225 checks composed extension blocks against their prototype,
including when views, substituted templates and the static registry. Omitted
annotations inherit; source-identical inherited/registry copies report once.
Static registries collect base declarations before extend contributions,
independent of their physical order.

`src/methods.rs` compiles first/also/only bodies into an arena-backed plan,
using an explicit work stack to avoid recursion for deeply extended methods.
Elaboration retains method-layer ranges for inheritance, synthesized when views
and substituted templates, distinguishing actual injected any_unit hooks. A
subtype replaces the virtual central body while ancestor wrappers remain.
Within one layer, only replaces the older subtree; first wraps it before and
also wraps it after. Return jumps skip only the current subtree, so outer also
bodies still execute. An owned program counter, argument save stack and result
slot survive TCM suspension alongside the retained frame and statement path.
First parameter rebindings feed the older subtree; also restores its incoming
arguments and receives the older result. Typed bodies get an implicit result
local with their inherited return type. Reference results default to NULL,
without allocating hidden objects. Normal completion returns the slot; bare
return uses it, while explicit return NULL stays NULL.

Atomic instance/static calls, inline value/compute calls, started TCMs, main
run, lifecycle hooks, creation-time init and inline sequence MAIN share the
plan. Existing atomic regions and constructor timing approximations remain;
this does not qualify full execution semantics against a reference simulator.

Remaining boundaries (the detailed behavior inventory is in STATUS.md):

- Full name resolution, signatures, flow-sensitive/bidirectional inference,
  operand narrowing and pre-elaboration type-expression checking. Explicit
  guard binding facts and retained type-expression scopes are implemented.
  The conservative
  checker leaves unknown/custom numeric kinds silent and set scalar domains
  untracked.
- Dynamic subtype matching for base-typed instances. When-qualified fields,
  chained modifiers and nested views compose, but a view's determinant pairs
  are not fully revalidated against base fields.
- General constraint solving: wide domains, division/modulo and temporal
  constraints exceed the bounded linear/polynomial solvers. Full connected
  field sets and select soft-order interactions remain incomplete.
- Further scoped callback handling, preserving static/on/evaluate restrictions.
  Scheduled instance value calls now retain expression/callee continuations,
  including runtime-resolved receivers. Temporal histories still share one global
  timeline; per-instance/event-specific compound timelines, edge-width
  legality, real/MVL/custom-numeric edges and distinct same-cycle samples
  remain gaps. Dotted physical event identity is resolved; indexed temporal
  paths and rebinding event references during active attempts remain gaps.
  Precise e/HDL event ordering remains unfinished.
- The remaining coverage semantics: composite-valued predicates, mixed
  composite classification, real/enumeral ranges, full option/domain typing,
  general constant/type resolution and radix/never behavior. Derived names and
  range labels evaluate literal-string concatenation/grouping with the same
  escape decoder as runtime strings. Name validation consumes the whole option
  expression; computed names share collection and graph lookup, and invalid
  values report E0224. A post-composition pass in coverage/constants.rs
  resolves supported initialized plain integer/string const fields in schema
  options, preserving reference spans. It also folds their pure initializers
  for instance/declaration-time storage, including forward/inherited fields
  and qualified static consts. Only names, ranges, goals and weights have
  option tokens rewritten; runtime filter expressions retain their scopes.
  Successful field values are memoized. Cycles and an active dependency plus
  expression-depth limit of 64 stop resolution; concatenated strings are
  capped at 256 KiB. Checked untyped arithmetic and the shared direct-operand
  precision helper avoid overflow/preserve supported 32/64-bit contexts.
  A shared scalar-representation resolver follows alias chains and evaluates
  supported width expressions in the owner scope. Explicit int/uint widths
  1–127 and star widths participate; sized initializers must already fit.
  The runtime precision lookup uses the same resolver for direct locals,
  fields and qualified static fields, keeping aliased/sized 32/64-bit and
  wide contexts aligned with schema evaluation. General storage narrowing,
  unresolved widths, enumerals, dynamic initializers and wider constant
  expressions remain gaps. Existing E0224 diagnostics reject unresolved
  schema values; unsupported fields shadowing predefined constants report
  E0224 explicitly rather than silently selecting the predefined value.
  Basic and derived
  scalar effects, source graphs, filters, goals and bounded domain proofs run
  as described above; the coverage subset of set_config is wired. Other
  configuration categories/options and set_config_max remain gaps.
- Numeric widths beyond the implemented static precision contexts, wider
  storage than the i128 model and evaluation of MVL literals. Real-valued set
  membership and full set-domain validation also remain unsupported.
- detach/delay/exec temporal expressions; select choice grammar beyond the
  evaluated constant-expression subset; inline sequence execution without
  enforcing do-keeping blocks; event-port behavior beyond hdl_path-bound edge
  sampling (MVL edge spellings and bind connections remain unmodeled).
- Rescanned/repeated preprocessing substitution, backtick preprocessor names
  and cross-import include-order visibility. Replacement rules currently apply
  once per file. Computed macros cover only the documented body subset; method
  calls, is-a, else-if chains and list/number bodies decline. Match repetition
  and category validation remain incomplete.
- Template when-subtype instances, extension members beyond the implemented
  fields/constraints/methods/on subset and subtype propagation of applied
  extensions. Original template actual-category validation remains excluded
  under 8.7 NOTE 2.
- The full Part 2 routine library/reflection model and declared-type range
  modifiers. Set literals, set_of_values/full_set_of_values and the selected
  set/list pseudo-methods already execute; their presence does not imply the
  rest of the library is implemented.
- Simulator bindings beyond xezim (including VHPI/VHDL), backend selection,
  browser engines beyond Chromium. The browser serializes native basic and
  derived coverage summaries, preserving duplicate combination labels and
  decimal-string counts; detailed bucket identities are not exposed. The single-source WASM ABI and worker runtime have real browser
  tests; this is an experimental declared-subset API, not a security audit.

## Qualification runner

`scripts/qualify.py` is a Python standard-library process boundary around the
native CLI and an explicitly configured local reference adapter. A manifest
identifies self-contained source cases, seeds, clause references and independent
expected values. Both backends use the same staged source snapshot; source
mutation and mismatched adapter case/hash/seed identities fail the comparison.
Native markers require a completion sentinel and an exact result map. A
reference adapter must report execution success plus tool/version/host-profile
metadata, which remain operator-supplied assertions rather than attestation.

Reports contain owned case values, fingerprints and status categories, not
raw process output or command arguments. Native-only mode explicitly records
reference-not-run and has a separate scope. Protocol fixture processes in the
Python unit tests exercise failure handling; CI separately builds the real
native executable and checks the three candidate programs. No qualified
Specman adapter, imported dependency bundle or actual differential result for
this new corpus is provided yet. See qualification/README.md for the adapter
contract, timeout/cleanup boundaries and qualification limits.

## The 2019 migration stages (PRs #40–#49)

Design notes for the IEEE 1647-2019 feature phases; per-phase honest
limits live in the PR bodies and the module headers.

- **Static storage (6.9.3, 17.2)** — `Interp::statics` is a per-type
  `BTreeMap<TypeName, BTreeMap<FieldName, Value>>` created at program
  start; keys come from elaboration's static-member registries (the
  declaring base or `extend` target). `Type::member` validates the
  declared-in-exactly-T rule (E0307). Bare field writes use the same owner
  walk as reads even without an instance receiver; locals and actual
  instance fields take precedence. Alias defaults recursively use their
  declared representation; more than 64 aliases produce E0402 rather than
  a fictitious alias object or unbounded recursion. `when` subtypes never
  own store keys; static `on` blocks compose per type and run with no `me`.
- **Interface attachment (6.13)** — an `interface` composes to its own
  ElaboratedType whose methods are the transitive signatures; a
  struct's `implementing` set composes through the same walk as like
  chains and extends (when-views clone it). An interface-typed value
  IS the implementing instance — dispatch resolves on the actual type,
  with E0414 when the method is missing. The conformance checks
  (E0311/E0312) run in the well-formedness pass the fixture contract
  observes.
- **Template parameters (8.2–8.7)** — `TemplateParam` unifies type and
  value parameters (kind/category/default); one binder
  (`bind_template_actuals` + `eval_const_expr`) serves the
  monomorphizer and the well-formedness pass; canonical instance names
  are computed after binding (defaults filled, value actuals
  normalized); `template extend` matches per entry (parameter forms
  bind tags, concrete entries match by name/value) gated by
  `conditions`.
- **Range-generated fields (6.9.7, 10.3.13)** — `gen` excludes `~`
  fields like `!` fields; after the other fields are concrete, each
  rgf field's reduced set is the intersection over its supported
  constraint shapes of the implied intervals (shape tree: `&&`
  intersects, `||` unions, implications contribute per guard); the
  concrete value is the set's minimum; per-instance `RgfState`
  metadata backs `rgf=` and the 29.13 routines.
- **chosen_from (10.5.8)** — `Expr::ChosenFrom` (the `read_only`
  wrapper unwrapped at parse); gen substitutes each list element into
  the target slot, evaluates the other constraints per candidate, and
  picks uniformly with the seeded RNG; contradictions report E0417.
- **Custom numerics (5.2.15, clause 31)** — `Value::Custom { type_name,
  instance }` with value semantics (`bind_slot_value` mints a fresh
  instance and runs `numeric_assign`); operator/conversion/output
  dispatch routes to the implementing struct's `numeric_*` methods
  (E0418 when missing); the 5.6.3 precision context classifies operand
  static types (Real/Long/>32-bit/≤32-bit) and truncates in the 32/64
  modes.
- **Execution phases (Table 1, 9.13)** — setup → generate (pre/
  post_generate) → connect_pointers → connect_ports (E0420-gated
  `do_bind`) → check_generation → the 13.2.2 scheduler → run DFS →
  extract → check → expect/assume → quit/finalize; the recognized
  2019 library surface that has no model yet reports E0419 from the
  `UNIMPLEMENTED_2019` table.
- **Tables (26, 32.3)** — `src/tables.rs` is a token pre-pass between
  `#define` replacement and macro expansion; expansions emit one brace
  group per data row with `<#…>` substitution (stack depth = number of
  `#`s, string literals included); `TableValue`/`TableRowValue` shared
  handles back the procedural API; file-read tables are cached and
  read-only until copied.
- **Annotations (35)** — `Annotation { name, attrs, span }` records on
  the node it precedes (declarations and the nine attachable member
  structs); attachments have no semantic effect; elab collects the
  registry `@T.get_all_annotations()` reads; the same-module rule is
  enforced per file (every file is one module here).
- **Reflection (32.1–32.2)** — the catalogue (`docs/REFLECTION_2019.md`)
  registers 50 method names + 43 type names under E0419; a user method
  of the same name always wins; `instance_iterator` is a
  `Value::InstanceIterator` shared handle whose `start` computes the
  full walk once (pre-order DFS, lists flattened, visited-set, depth
  cap 64).
