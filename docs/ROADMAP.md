# Roadmap

The IEEE 1647-2019 migration phases below are complete; the broader
milestones remain partial. Recent work adds conservative type checking
(now including coverage items), bounded polynomial constraint solving
and streaming temporal rule monitors. Remaining work includes full name
resolution/type inference, temporal edge expressions, call-stack
continuations for suspended TCM calls, the remaining coverage options,
general constraint solving, differential qualification, HDL event
ordering and a browser sandbox API. Completed migration checkboxes do
not imply full language support or IEEE conformance.

## M0 — Repository and infrastructure
- [x] Rust package, README, license, tests, CI definition
- [x] First tokenization smoke tests
- [x] Verify CI and tests on GitHub (fmt/clippy/test green on ubuntu-latest
      since PR #2)

## M1 — Language frontend
- [x] Expand lexical grammar: keyword recognition, e operators, sized/unsized
      based numerals, `0x`/`0b` literals and reals, with golden fixtures and a
      set-equality cross-check against the MIT-licensed reference grammar
      pinned under `external/`. `<'` / `'>` are code-segment markers
      only in the first column of an otherwise blank line; prose outside
      those segments is skipped, and a file with no begin marker is all
      code. Decimal `K`/`M` multipliers, `0c"..."` character literals,
      and MVL digits in non-decimal based literals are tokenized.
      Not yet the complete lexical grammar: MVL literals are not
      evaluated, and the 32-bit default width is not applied
- [x] Build AST and parser for a declared subset: `struct`/`unit`/`extend`/
      `type` with `like` (when-modifier prefixes recorded on the
      declaration; user `extend global` is rejected per clause 4.4.2.2),
      fields (name groups, `%` physical and `!` do-not-generate),
      scalar/list/named types, enumeral types, `keep`/`keep soft` and
      `keep bind`, `when` subtype members (`value'id` modifiers),
      `simple_port`, `event` (trigger stored as written), `on` members
      (clause 11.2: dotted event path and action block; extension is a
      repeated declaration), `import`, literal `define`, macro
      definitions (clause 15.2/15.3:
      `define <name'category> "match expression" as [computed] { body };`
      — header, pattern and raw body tokens recorded; the
      match-expression grammar and expansion are later milestones),
      scalar type declarations (`type name: <type>;` — the underlying
      representation recorded), and
      `sequence ... using` (synthesized into a type, a driver unit and a
      kind enum). Methods have parameter lists,
      optional `@event` TCM suffixes, and statement bodies: local `var`,
      assignment (including quoted HDL paths), calls, indexing, `return`,
      braced `if`/`else if`/`else` (optional `then`), `while` with
      `break`/`continue`, `for each` (including `using index (idx)` and
      `in reverse` bindings) and `for i from .. to [down] to`, `case`,
      `check that`, `wait`, `emit`, `start`, `compute` (executed inline
      like `start`), sequence `do`, `print`, `gen`, `stop_run`. Method
      bodies may be `is empty` or `is undefined` (both body-less);
      parameters may be untyped. Expression precedence matches clause 4.6
      Table 12, including `is [not] a <type> [(name)]` (runtime check is
      exact-name or `like`-chain; a true match binds `(name)`) and the
      `~` / `!~` string-match operators (runtime
      evaluation is the case-insensitive full-string literal subset;
      meta-characters and `/.../` AWK patterns report `E0410`).
      Template type declarations (clause 8.2) parse: `template
      [package] struct|unit name of <params> [like base] { .. }` with
      `<tag'category>` parameters and defaults — formals
      (`<type>`), template references (`deque of int`) and keyed lists
      (`list (key:it) of t`) are recorded in type positions.
      Template extensions (clause 8.6) parse too (`template extend
      name of <params> { .. };`, the extension's own parameter list
      recorded). Instances monomorphize (clause 8.7): every used
      `name of (actuals)` materializes a struct named by the canonical
      rendering of its actuals, with the actuals substituted for the
      formals in field types, method signatures and method bodies
      (type positions inside actions — `new <template> of <type>`,
      casts, `is a` checks, local types — substitute, and every
      instance a body names joins the worklist), trailing
      parameters filled from their defaults (defaults referencing
      earlier formals substitute through), and the parameterized base
      substituted to the base chain; identical actuals share one
      instance, and template-typed fields generate and dispatch under
      it. Each template extension applies to an instance when every
      actual complies with the extension's parameter specifications
      (8.7's NOTE 3): the prose-named `scalar` and `enumerated`
      categories check the actual's shape (8.2.2's full category table
      is an image, so any other spelling reads as the specific-type
      form and can only ever mismatch), the original template's own
      parameter categories stay unvalidated (8.7's NOTE 2), and an
      extension with a mismatched parameter count reports `E0304` and
      does not apply — a complying extension's fields, constraints,
      methods and `on` blocks substitute the extension's formals with
      the instance's actuals and append. Typed `new
      <named-or-template-instance> [with { .. }]` instantiates and
      runs the `with` assignments (clause 6.3); untyped `new` still
      evaluates to an
      explicit not-supported diagnostic. Coverage groups (clause 14.2)
      parse, are recorded, and collect: `cover <event> is [also]
      { item/cross/transition entries; option entries }` and `cover
      <event> is empty;` — item types and sampled expressions are
      structural, every `using` option clause and group option entry is
      recorded verbatim. The coverage data model (`src/coverage.rs`)
      merges `is also` extensions into their base group, parses
      `using ranges = { range([lo..hi], "label"); .. }` into explicit
      buckets, applies the 14.3 defaults (16 integer buckets, two
      boolean buckets), samples item values when the group's event
      fires, and grades covered-buckets-per-item (`run --coverage`
      prints the report). Derived items collect too: `transition item`
      derives previous→current pairs from the base item's consecutive
      samples (14.5, n samples → n-1 transitions, graded over the
      base's ordered bucket pairs) and `cross a, b` records every
      combination of the crossed items' bucket labels (14.4, graded
      over the cross product); unbucketed sources count without
      grading. Per-instance coverage grades per bucket with holes
      (14.3.2), the option entries the prose defines apply
      (`at_least`/`weight`/`per_instance` at item and group level,
      `no_collect`/`instance_no_collect` hiding the un-presented
      group/item per 14.2.2/14.3.4, the cross/transition `name`
      option per 14.4/14.5), and the 14.9 configuration is honored
      through the builder API (`max_int_buckets`, the coverage mode);
      runtime configuration enables collection. Per-unit-instance
      groups and group/basic-item `when` filters run; full option type
      checking, real/enumeral ranges, derived-item ignore/illegal filters and radix
      remain unfinished.
      Predefined non-scalar types `file` and `event` are accepted in
      type positions; `for each <type> (v) in list` records the element
      type filter. Simple macros expand (clause 15): `define <name'
      category> "pattern" as { body };` compiles the match expression
      (15.4 — escaped literal terminals, `<[tag']type>` non-terminals,
      `[ ]` option, `( )` grouping, `|` alternation) and a pre-pass
      over the token stream expands every invocation site with
      backtracking matching (15.5.2) and parameter substitution
      (15.6), later definitions taking priority (15.5.1); bodies may
      invoke other macros, and computed bodies (15.3) evaluate at
      match time over a restricted statement subset (`var`/
      assignments, braced `if`/`else`, `return`, the implicit
      `result`; string parameters (their matched text), string
      literals, `+`, `==`/`!=`, calls) — `reject_match()` and any
      unevaluable construct decline the definition so the next
      lower-priority definition applies, and the returned string
      tokenizes into the expansion. The v1 approximations: literals
      match one token each (whitespace literals vanish), the
      repetition form matches as one non-terminal, categories are
      not validated, and computed bodies beyond the subset (method
      calls, `is a` checks, `else if` chains, list/number
      expressions) decline.
      `expect`/`assume` members (clause 11.4) parse their
      temporal expressions into the clause 12 grammar
      (`TemporalExpr`: `@event` atoms, `cycle`, `true(exp)`,
      `change/rise/fall(exp)` edge samples, `{;}` sequences, `=>`
      yield, `and`/`or`/`not`/`fail`/`eventually`, `[n]*`/`~[l..h]*`/
      `[l..h]*` repeats, and the `te @event` sampling operator;
      `detach`/`delay`/`exec` report `E0120`) with the `else` failure
      body parsed as ordinary actions, and temporal `wait` statements
      parse the same grammar (13.2.4, including the readability-only
      `until`), as do the `sync` synchronization actions (13.2.3, bare
      and TE forms). The temporal-engine core
      (`src/temporal.rs`) propagates the sampling event to sampled
      normal form (12.1.4) and evaluates tight satisfaction (12.1.1.2)
      and last-point success (12.1.1.12) over explicit finite paths
      (12.3) through caller-supplied expression callbacks — the
      interpreter drives it over the run's event log (see M3); a
      simulation clock with TCM suspension is a later milestone.
      `select { weight : choice; .. }`
      distribution
      expressions (clause 10) parse — weights structurally, choices
      verbatim — and the rejection sampler applies them (10.3.12.3):
      an arm is chosen with probability weight/(sum of all weights)
      over the field's current range, and `min`/`max`/`edges`/`others`
      and `normal(center, stddev)` map the choice to a value; choices
      evaluate constant integer expressions over the recorded tokens
      (`MAX_UINT`/`MAX_INT` included), and unevaluable choices leave
      the field unwidened. In action position (`var x = select ...`)
      the select still reports the honest not-evaluated diagnostic.
      Preprocessor directives (clause 21): `#ifdef` / `#ifndef` /
      `#define` / `#undef` / `#else` in statement, struct-member and
      action positions — `#define` records the flag and applies the
      21.3 replacement rules as a token pre-pass (the replacement
      text substitutes for every occurrence of the name in subsequent
      code, except inside string literals and the directives
      themselves; single-pass, per-file), `#else` flips the included
      side of a conditional (both `} #else { .. };` and `}; #else {
      .. };` spellings; the 21.3 syntax examples are images in the
      local redline copy, so only the prose is implemented), and
      excluded branches are skipped without parsing. Spanned
      diagnostics and declaration/member/statement-level recovery. The
      CLI resolves `import a.b` to `a/b.e` or `a.b.e` beside the source
      and checks the merged declarations (not yet: clause 8.4's
      template when-subtype instances, extension members beyond
      fields/constraints/methods/`on` blocks and the 8.6 subtype
      propagation of applied extensions (8.7's NOTE 2
      actual-category validation stays out with the original
      template's), the full match-expression grammar (clause 15.4's repetition and
      category validation — computed macros evaluate their restricted
      body subset, see M1), the remaining clause 14
      coverage model —
      real/enumeral ranges and general constant/type resolution,
      `radix`/`never` and derived-item filters; full option type checking;
      runtime configuration categories/options
      beyond coverage mode, `max_int_buckets`, `at_least` and
      `check_illegal_immediately`, and `set_config_max()` —
      the clause 12/13 temporal
      engine's remaining simulation integration — the scheduler runs
      started TCMs concurrently and waits/sync suspend, `@`-triggered
      TCM invocation gates the entry on the declared sampling event's
      occurrence (17.1.2), and `@sim`/event-port edge sampling
      (11.1.4.2, 9.7.3) fires at the scheduler's cycle boundaries
      through a real session or the injected signal-history test seam —
      suspension re-entry reaches every compound statement now (waits
      inside `if`/`case`/`for`/`for each`/`repeat until` bodies suspend
      and resume through the statement-path cursor), while re-entry
      into inline method calls and `compute` (a per-coroutine
      call-stack continuation behind 13.2.2's return-resumes-callee
      boundary) is not wired —
      `detach`/`delay`/`exec` TEs, select
      choice policies beyond the constant-expression subset (the
      full choice-expression grammar and soft-order interactions),
      `$1`-`$27` match captures,
      the preprocessor's rescanned/repeated substitution (replacement
      rules apply in one pass; the backtick Verilog-style names of
      4.1.5.4 and the cross-file include-order visibility of B.5 are
      not modeled)
- [~] Add symbol resolution, types, diagnostics — well-formedness
      checks (duplicate declaration/field names, duplicate
      parameters/local variables/enumerals, unknown named/`like`/`extend`
      types including method signatures and `list of` elements, unknown
      `keep` identifiers checked against fields and enumerals, zero
      widths). The check runs on the import-merged declaration list.
      The post-composition conservative type pass (`src/typing.rs`)
      checks methods, field initial values, constraints, `on` bodies and
      coverage items, including synthesized when views. E0220–E0223
      report definite incompatible values, missing members, non-list
      indexing and invalid operands; unknown types stay silent.
      Declared parameters bind within each method; local `var x := e`
      declarations propagate known initializer kinds, homogeneous list
      element kinds and equal-kind conditional branches (clause 18).
      Scalar/list aliases resolve with bounded recursion. Unknown or
      ambiguous kinds and custom numeric dispatch remain conservative;
      there is no flow-sensitive or interprocedural inference. Coverage
      options remain unchecked raw tokens and full name resolution is
      still missing
- [ ] Differential corpus against an authorized reference simulator where
      available (started: Specman reference runs via EDA Playground are
      recorded in docs/VERIFICATION.md; no automated differential harness yet)

## M2 — Elaboration and randomization
- [~] `extend`, `when`, units, structs and inheritance — elaboration
      started: `like` chains and `extend` blocks fold into one composed
      member list per type (`src/elab.rs`, `e1647 elab`), with
      inheritance-cycle, cross-declaration duplicate-field and
      unmodified-method-redefinition diagnostics. `simple_port` members
      carry HDL paths taken from `hdl_path()` constraints, and
      `sequence` declarations are recovered into `Elaborated.sequences`.
      `on` members compose through the same fold, base types first and
      extends in declaration order — `on <port>$` members (clause
      11.3) included: event ports declare as `name: <dir> event_port
      [is instance];` (9.5), the `$` access operator names the port's
      event (9.5.2), `emit p$;` triggers the port, and the 11.3
      restriction holds — wait/sync/start/compute inside an `on`
      action block report `E0210`. `when` subtypes (6.7) resolve:
      each block (`when BUSY'kind`, the corpus's `when NO_CONSTRAINT
      env` space form, or a chained `when LO'kind HI'sel`) synthesizes
      a subtype view named with its values space-joined before the
      base (`LO HI pkt`) — the base's composed members with the when
      block's appended, marked with every (field, value) pair — and
      when-qualified field types (`f: LO pkt`, chained
      `f: LO HI pkt`) instantiate that view, so the when members
      (fields, constraints including select distributions, methods, on
      blocks) apply for those instances; a `when` member nested inside
      a when block applies on top of the enclosing view (a further
      combined view, budget-capped); `is a` checks walk the view's
      base chain. Base-typed instances do not dynamically match
      subtypes yet; the conservative type pass checks composed views
- [~] Constraint IR for `keep` expressions — started: `src/solve.rs`
      translates conjunctions of linear comparisons over bounded integer
      fields into a solver problem; range membership (`x in [lo..hi]`)
      translates to the two comparisons; unsupported shapes fall back to
      rejection sampling. Disjunctions (item-list membership, `||`) and
      negation trees translate; bounded polynomial problems of degree
      up to four use feature-independent enumeration with capped domain
      products. Wide domains, division/modulo and global constraints
      remain unsupported by the solver. `keep bind` and
      `hdl_path()` / `delayed()` constraints do not gate `gen`.
      Connected field sets (10.3.6-10.3.7) scope generation, v1: `gen <obj>`
      inside a method gates the object with the enclosing instance's
      constraints that reference it — `p.v == select { .. }` rebases to
      `v == select { .. }` (depth-two paths; other shapes drop until
      full CFS lands), so when-view distribution policies drive field
      generation
- [~] Pluggable SMT/constraint solver — started: the Pumpkin CP solver
      (pure Rust, consol-lab) is integrated behind the `solver-pumpkin`
      cargo feature and is deterministic per seed; CI verifies the feature
      on ubuntu. Candidates for further/alternative backends: z3.rs (needs
      the native Z3 library), oxiz (young pure-Rust SMT), rustsat (SAT
      toolkit). Note: the crates.io `pcp` is an unrelated Post
      Correspondence Problem searcher, not a CP solver
- [~] Reproducible seeded randomized objects — started: `gen` is a seeded
      rejection sampler by default and, with `solver-pumpkin`, a seeded
      Pumpkin solve; values may differ between backends for the same seed
      (different algorithms), but runs are deterministic per backend. The
      rejection sampler narrows each field's sampling domain to the
      intersection of the type's constant range/equality constraints
      (without this, a constrained unsized `int` is unhittable in 1000
      attempts). Data fields are implicitly generated before `run()`
      (skipping `!` and unit-typed fields; `keep list.size() == N` sets
      list lengths)

## M3 — Behavioral execution
- [~] Interpreter and method calling conventions — started: the
      statement subset in `src/interp.rs` executes expressions, `var`,
      assignment, calls, `if`/`while`/`for`/`case`, `repeat until`
      (clause 20.2.3 — the body at least once), `break`/`continue`,
      `return`, `print`/`out`/`outf`/`message`, `check that` with a
      DUT-error count, `pack`, `append`, lists with the clause 27
      pseudo-method subset (the 27.3 modifiers
      `push`/`push0`/`add`/`add0`/`pop`/`pop0`/`insert`/`delete`/
      `fast_delete`/`clear`, the no-argument general forms
      `size`/`is_empty`/`exists`/`copy`/`reverse`/`top`/`top0`, the
      `min`/`max`/`min_index`/`max_index` extremes with optional per-item
      key expressions, the
      no-argument `sum`/`product`/`and_all`/`or_all` aggregates with
      their stated empty-list results, and `is_a_permutation`; the
      numeric expression aggregates and the predicate queries `has`,
      `count`, `all` and `all_indices`; other expression-taking forms
      remain documented gaps reporting `E0405`),
      struct instances, sequence `do` (generates the item in place; the
      `keeping` block is not enforced), and `gen`. The optional `do`
      sugar on `if`/`while` — including the one-statement
      `if cond do <statement>;` form — parses and runs. Entry point is
      the built-in `sys` unit's `run()` methods, and implicit
      generation plus
      a MAIN sequence start also run when no explicit `run()` exists.
      Integers are stored unbounded `i128`; the 5.6.3 precision rules
      (the 2019 migration's Phase 7) truncate binary results to the
      32/64-bit modes where the operand static types pin them (untyped
      operand pairs stay unbounded). No
      event-driven scheduling
- [~] Events, temporal expressions and time-consuming methods — started:
      event triggers are stored, `emit` records a firing and executes the
      event's `on` action blocks inline, in composition order (clause
      11.2; each block with a fresh locals frame on the owning
      instance), and `compute` calls a TCM synchronously. The clause
      13.2.2 scheduler runs concurrent TCMs: `start` enqueues a
      coroutine with the implicit sync cycle before it commences, and
      coroutines take round-robin turns that run atomically from one
      synchronization action to the next — `wait`/`sync` suspend (with
      the captured locals frame and a skip-cursor that re-enters `while`
      chains mid-iteration), and a cycle passes when nothing is
      eligible. The clause 12 temporal-engine core
      (`src/temporal.rs`) is driven by the run loop's event log: `emit`
      records occurrences in per-cycle states, temporal `wait`s
      (`@event`, sequences, propositions, event repeats, with 13.2.4's
      readability-only `until`) re-evaluate at every turn against the
      grown log, a temporal wait unsatisfied for 1,000 cycles (or the
      5,000-cycle shared budget) reports `E0409`, and
      `expect`/`assume` rules (elaborated per type) stream through
      per-rule monitors at finalized cycle boundaries; invariants fail
      when violated, and obligation verdicts finalize when their windows
      close (12.3.3). A failed rule runs its `else` body (or prints the
      failure when there is none) as a check error. One global
      timeline. Proposition and repeat-bound values are retained per
      state and evaluation context: rules sample before a global cycle
      closes and at quit, and waits sample at each check, preserving
      their local context. Later matching uses recorded values. Raw
      `change`/`rise`/`fall` expressions and event-specific/per-instance
      histories remain gaps. Suspension re-enters compound
      statements, including `if`/`case`/`for`/`for each`/`repeat until`;
      inline calls and `compute` still need call-stack continuations,
      and `on` blocks remain atomic
- [~] Coverage data model and reports — started: `src/coverage.rs`
      builds buckets and grades from the recorded groups (14.3's
      ranges/defaults, 14.6 merging), the interpreter samples on the
      group's event, derived items collect (14.5 transitions as
      previous→current pairs, 14.4 crosses over the sources' bucket
      labels), `per_instance = TRUE` grades per bucket with holes
      (14.3.2 — the group grade is the weighted instance grades plus
      the per_type bucket), `at_least`/`weight` apply from the option
      tokens (the group-level spellings included, 14.9.3/14.9.4), the
      `no_collect`/`instance_no_collect` options hide the group/item
      from reports while retaining sampling storage (14.2.2/14.3.4), the
      cross/transition `name` option renames the derived item
      (14.4/14.5), and `run --coverage` reports instances; the 14.9
      configuration is honored through the builder API
      (`max_int_buckets`, the coverage mode) and runtime `set_config(cover, ...)`.
      Runtime collection starts disabled until enabled explicitly.
      Per-unit scoping follows the nearest requested unit in the field
      tree, including inherited units, flattened lists and unsampled
      instances. Group/basic-item `when` filters run in the owner context;
      hidden sources remain available to derived items. Full option type
      checking and remaining option semantics are unfinished
- [~] Coverage configuration options (14.9) and per-unit-instance options (14.2.2) —
      started: the configuration record (`CoverageConfig`: the
      coverage mode per 14.2's NOTE, `max_int_buckets` per 14.3's
      bracket) is honored through the `build_with_config` builder API,
      and through `set_config(cover, ...)` (29.9). Modes `normal` and
      `count_only` collect; `off` pauses without clearing counts;
      Boolean values also control enablement. `max_int_buckets` accepts
      0–65536 before sampling; after sampling, changes report E0402.
      `per_unit_instance [= unit_type]` and `instance_no_collect` with
      `inst` collect and present per-unit data (14.2.2/14.3.4); structural
      option/scope errors are E0224. Other runtime configuration
      categories/options, `set_config_max`, derived-item ignore/illegal filters,
      real/enumeral ranges, general constant/type resolution, radix/filter semantics and full option type checking
      remain gaps. Integral range widths and per-range goals run with
      range/item/group/global precedence; zero-goal buckets are ungraded
      and do not show holes. Explicit construction is capped at 65536
      buckets per item; checked i128 arithmetic diagnoses unsupported or
      overflowing parameters (E0224). Global at_least changes are allowed
      before sampling and report E0402 afterwards. Basic-item ignore/illegal
      and per-unit instance variants exclude observations and whole buckets;
      illegal values produce immediate or deferred DUT errors. Interval masks
      and bounded arithmetic classification preserve derived bucket identity.
      Cross/transition when, visibility, hit-goal and weight options run over
      basic and previously declared derived sources. Derived grades enter the
      weighted per-type aggregate in
      mixed per-instance groups; filtered transitions retain source history.
      Derived source graphs preserve nested key identities, current-round
      freshness, masks and per-unit histories. Source order/name/arity errors
      are E0224; expanded tuples are capped at 65536 scalar components.
      Derived ignore/illegal predicates remain gaps.
      Exact overlap/phase/filter-side-effect behavior needs qualification
- [~] Part 2 predefined routine library — started with the reflection
      set: `set_of_values(<type>)`/`full_set_of_values(<type>)`
      (5.10.5/5.10.6 — the argument parses as a type) build the
      `Value::Set` of all legal values (5.2.12 canonical intervals:
      sorted, merged, non-neighboring), the 28.5 set pseudo-methods
      query and combine (`size`/`uint_size`/`min`/`uint_min`/
      `int_min`/`max`/`uint_max`/`int_max`, the `_is_uint` guards,
      `union`/`intersect`/`diff`), `to_string()` renders scalars and
      sets, and `as_a` to an enumeral type maps the integer index to
      the enumerand (5.10.2). The clause 27 list pseudo-methods run the
      no-expression subset on list values (the 27.3 modifiers, the
      no-argument general forms and aggregates, numeric expression
      aggregates, and extrema with optional key expressions bound to
      `it`. Keyed extrema select the original item/index, prefer the
      latest tied item and snapshot traversal; implicit index bindings
      remain unsupported. `has`/`count`/`all`/`all_indices` accept one
      boolean expression bound to `it`, preserve order and return fresh
      filtered/index lists; implicit index bindings are not modeled.
      Other expression-taking forms, `resize`, the 27.6 CRC family and
      27.7 keyed lists are documented gaps in `src/interp.rs`). Set literals
      in expressions, range
      modifiers on declared types (so `full_set_of_values` can
      differ), and bare scalar keywords as type arguments
      (`set_of_values(int)` — needs type objects in expression
      position) are not modeled

## M4 — Simulator integration
- [~] HDL bridge — started: `src/hdl.rs` spawns `xezim` with the VPI
      control bridge in `c/xezim_bridge.c` and speaks a line protocol
      (`SET`/`GET`/`RUN`/`FINISH`). The interpreter exposes `hdl_load` /
      `hdl_set` / `hdl_get` / `hdl_run` / `hdl_finish` and routes quoted
      HDL paths (`'top.count'`) through that session (`E0408` when no
      simulator is loaded). The binding sits behind a `Simulator` seam
      (`load`/`name`/`set`/`get`/`run`/`finish`): the xezim VPI bridge is
      the `XezimSimulator` backend (native targets — under wasm32 it is
      not compiled), the `NullSimulator` is the no-op backend where no
      simulator exists (its operations report the honest no-simulator
      error, and it is the tests' dispatch double), and a third backend
      (VHPI — not implemented) plugs in with one `Simulator` impl and one
      registry arm. Still one real simulator binding, and no backend
      selection knob yet (`hdl_load` resolves through the default
      backend)
- [~] SystemVerilog integration tests and event ordering — started: the
      compose `mixed` service runs `examples/mixed_counter.e` against
      `examples/mixed/counter.sv`. Event ordering between e and the HDL
      simulator is not implemented
- [~] WASM build and browser sandbox API — started: the core library
      (lexer/parser/elaborator/interpreter) compiles for
      `wasm32-unknown-unknown` (CI runs `cargo check --target
      wasm32-unknown-unknown`); nothing had to be excluded from the
      build, and the process-spawning xezim backend is native-only by
      cfg there (`hdl_load` under wasm32 fails with the honest
      no-simulator error). No browser sandbox API and no wasm runtime
      test yet

## Migration to IEEE 1647-2019

The implementation target moved from IEEE 1647-2016 to **IEEE 1647-2019**.
The task plan (per-task specs, syntax shapes, test cases and the diagnostic-code
allocation) lives in
[e1647-rs_IEEE1647-2019_migration_plan.md](e1647-rs_IEEE1647-2019_migration_plan.md);
this section tracks what has landed. Checkboxes move in the same PR that lands
the work.

- [x] Phase 0 — Groundwork: the clause-number sweep 2016 → 2019 across
      `src/`, `tests/`, `README.md`, `docs/` (per the plan's §1.4 map; the
      few sub-clause numbers the plan does not pin were shifted by the same
      insert-an-Introduction mechanism and are flagged in the PR), Table 8
      keyword alignment (the 2019 additions `annotation`, `chosen_from`,
      `conditions`, `implementing`, `in_table`, `interface`,
      `numeric_type`, `repeatable` are keywords; the remaining Table 8
      words stay contextual, each documented in `src/lexer.rs`), and the
      2019 designation in `README.md`, `AGENTS.md`, `docs/REFERENCES.md`
      and the verification-log header
- [x] Phase 1 — Lexical/syntactic foundation (T1.1–T1.13): standard
      multi-line comments (span lines and code segments, non-nesting —
      4.2.4), statements spanning code segments (4.2.3), brace-grouped
      statements and members (4.3.2/4.3.3), `longint`/`longuint` with
      E0207 for subtypes of either (5.2.3.1), the Table 12 row order
      pinned by a precedence test (4.6), `list_ptr==`/`!list_ptr=` over
      reference-identity lists (4.11.4, E0411), `in_table` (4.11.8,
      E0121/E0412, gen narrows to the rows' projection), `evaluate`
      (4.17.7, E0122/E0123), `typeof()`/`typeof_item()` in type position
      (5.9, E0208/E0209), subtype qualifiers in dot access (4.17.3,
      E0413), field initial values (6.9, E0211/E0213/E0214; E0212 waits
      for Phase 2 with the contextual `static` prefix), the 4.4.2.2
      `extend global` rewording, and the 4.4.5.3 literal-`global`
      ordering recorded (the global struct itself is not modelled yet)
- [x] Phase 2 — Static members (T2.1–T2.4): static fields and `::` access,
      static methods and their restrictions, static events and `on
      Type::event`, keyed-list key restrictions
- [x] Phase 3 — Interfaces (T3.1–T3.5): `interface` declarations,
      `implementing`, interface types and casts, dynamic dispatch
- [x] Phase 4 — Templates rework (T4.1–T4.7): value parameters,
      `conditions` bounding, `implementing` in templates, `template
      extend`, derived defaults, template interfaces/numeric types
- [x] Phase 5 — Range-generated fields (T5.1–T5.4): the `~` modifier,
      reduced-set generation, `rgf=`, the rgf routines
- [x] Phase 6 — `chosen_from` (T6.1)
- [x] Phase 7 — Custom numeric types and precision (T7.1–T7.4):
      `numeric_type` + `numeric_i`, operator dispatch, six-way precision
      rules
- [x] Phase 8 — Execution phases and library surface (T8.1–T8.12):
      `extract`/`check`/`finalize`, generation sub-phases, port
      binding methods, unit queries, coverage API additions, message
      hooks, numeric list pseudo-methods, `deep_copy`, honest `E0419`
      stubs (`rerun()` included)
- [x] Phase 9 — Tables (T9.1–T9.3): the `table … with { … }` preprocessor,
      `table from` operators + `csv_to_table`, the `table_manager` API
- [x] Phase 10 — Annotations (T10.1–T10.3): `annotation @name`, attaching
      annotations, the annotation API
- [x] Phase 11 — Reflection catalogue (T11.1–T11.2): `E0419` stubs for the
      32.1 names, `docs/REFLECTION_2019.md`, `instance_iterator`
- [x] Phase 12 — Final docs/claims pass (T12.1)

Per-phase limits and approximations live in the PRs (#40–#49) and the
verification-log entries; the honest-gap summary lives in
[ARCHITECTURE.md](ARCHITECTURE.md) and the README status line.

All stages require standards-aware conformance testing; compatibility with commercial products is not assumed.
