# Roadmap

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
      evaluated, `/* */` block comments are an extension beyond
      clause 4.1.3, and the 32-bit default width is not applied
- [x] Build AST and parser for a declared subset: `struct`/`unit`/`extend`/
      `type` with `like` (when-modifier prefixes recorded on the
      declaration; user `extend global` is rejected per clause 4.3.1.1),
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
      parameters may be untyped. Expression precedence matches clause 4.5
      Table 12, including `is [not] a <type> [(name)]` (runtime check is
      exact-name or `like`-chain; a true match binds `(name)`) and the
      `~` / `!~` string-match operators (runtime
      evaluation is the case-insensitive full-string literal subset;
      meta-characters and `/.../` AWK patterns report `E0410`).
      Template type declarations (clause 8.1) parse: `template
      [package] struct|unit name of <params> [like base] { .. }` with
      `<tag'category>` parameters and defaults — formals
      (`<type>`), template references (`deque of int`) and keyed lists
      (`list (key:it) of t`) are recorded in type positions.
      Instances monomorphize (clause 8.3): every used
      `name of (actuals)` materializes a struct named by the canonical
      rendering of its actuals, with the actuals substituted for the
      formals in field types and method signatures, trailing
      parameters filled from their defaults (defaults referencing
      earlier formals substitute through), and the parameterized base
      substituted to the base chain; identical actuals share one
      instance, and template-typed fields generate and dispatch under
      it. Method-body references to other template instances and
      category-compliance checks (8.3's notes) substitute/verify in a
      later milestone, and `new [<type>] [with { .. }]` parses but
      evaluates to an
      explicit not-supported diagnostic. Coverage groups (clause 14.1)
      parse, are recorded, and collect: `cover <event> is [also]
      { item/cross/transition entries; option entries }` and `cover
      <event> is empty;` — item types and sampled expressions are
      structural, every `using` option clause and group option entry is
      recorded verbatim. The coverage data model (`src/coverage.rs`)
      merges `is also` extensions into their base group, parses
      `using ranges = { range([lo..hi], "label"); .. }` into explicit
      buckets, applies the 14.2 defaults (16 integer buckets, two
      boolean buckets), samples item values when the group's event
      fires, and grades covered-buckets-per-item (`run --coverage`
      prints the report). Derived items collect too: `transition item`
      derives previous→current pairs from the base item's consecutive
      samples (14.4, n samples → n-1 transitions, graded over the
      base's ordered bucket pairs) and `cross a, b` records every
      combination of the crossed items' bucket labels (14.3, graded
      over the cross product); unbucketed sources count without
      grading. Per-instance coverage and the option tables remain
      recorded, not collected.
      Predefined non-scalar types `file` and `event` are accepted in
      type positions; `for each <type> (v) in list` records the element
      type filter. Simple macros expand (clause 15): `define <name'
      category> "pattern" as { body };` compiles the match expression
      (15.4 — escaped literal terminals, `<[tag']type>` non-terminals,
      `[ ]` option, `( )` grouping, `|` alternation) and a pre-pass
      over the token stream expands every invocation site with
      backtracking matching (15.5.2) and parameter substitution
      (15.6), later definitions taking priority (15.5.1); bodies may
      invoke other macros. The v1 approximations: literals match one
      token each (whitespace literals vanish), the repetition form
      matches as one non-terminal, categories are not validated, and
      computed macros (15.3) stay recorded, not computed.
      `expect`/`assume` members (clause 11.4) parse their
      temporal expressions into the clause 12 grammar
      (`TemporalExpr`: `@event` atoms, `cycle`, `true(exp)`,
      `change/rise/fall(exp)` edge samples, `{;}` sequences, `=>`
      yield, `and`/`or`/`not`/`fail`/`eventually`, `[n]*`/`~[l..h]*`/
      `[l..h]*` repeats, and the `te @event` sampling operator;
      `detach`/`delay`/`exec` report `E0120`) with the `else` failure
      body parsed as ordinary actions, and temporal `wait` statements
      parse the same grammar (13.1.3, including the readability-only
      `until`), as do the `sync` synchronization actions (13.1.2, bare
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
      verbatim — and the rejection sampler applies them (10.2.12.2):
      an arm is chosen with probability weight/(sum of all weights)
      over the field's current range, and `min`/`max`/`edges`/`others`
      and `normal(center, stddev)` map the choice to a value; choices
      evaluate constant integer expressions over the recorded tokens
      (`MAX_UINT`/`MAX_INT` included), and unevaluable choices leave
      the field unwidened. In action position (`var x = select ...`)
      the select still reports the honest not-evaluated diagnostic.
      Preprocessor directives (clause 21): `#ifdef` / `#ifndef` /
      `#define` / `#undef` in statement, struct-member and action
      positions with flag semantics — `#define` records the flag but
      applies no text substitution, and `#else` is not parsed. Spanned
      diagnostics and declaration/member/statement-level recovery. The
      CLI resolves `import a.b` to `a/b.e` or `a.b.e` beside the source
      and checks the merged declarations (not yet: template `extend` (clause 8.2) and body-level instance substitution,
      computed macros and the full match-expression grammar (clause
      15.4's repetition and the 15.3 computed bodies — simple
      define-as macros expand, see M1), the remaining clause 14
      coverage model —
      the option tables beyond per_instance/at_least/weight, coverage
      configuration (28.9) and per-unit-instance options (14.1.1) —
      the clause 12/13 temporal
      engine's remaining simulation integration — the scheduler runs
      started TCMs concurrently and waits/sync suspend, but
      `@`-triggered TCM invocation, `@sim` sampling, and suspension
      re-entry beyond `while` chains are not wired —
      `detach`/`delay`/`exec` TEs, select
      choice policies beyond the constant-expression subset (the
      full choice-expression grammar and soft-order interactions),
      `$1`-`$27` match captures,
      `#define` replacement rules, `#else`)
- [~] Add symbol resolution, types, diagnostics — minimal well-formedness
      checks only (duplicate declaration/field names, duplicate
      parameters/local variables/enumerals, unknown named/`like`/`extend`
      types including method signatures and `list of` elements, unknown
      `keep` identifiers checked against fields and enumerals, zero
      widths). The check runs on the import-merged declaration list.
      No type system, and no subtype-aware scope yet
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
      [is instance];` (9.4), the `$` access operator names the port's
      event (9.4.1), `emit p$;` triggers the port, and the 11.3
      restriction holds — wait/sync/start/compute inside an `on`
      action block report `E0210`. `when` subtypes (6.6) resolve:
      each single-modifier block (`when BUSY'kind` or the corpus's
      `when NO_CONSTRAINT env` space form) synthesizes a subtype view
      named `VALUE base` — the base's composed members with the when
      block's appended, marked with its discriminator — and
      when-qualified field types (`f: LO pkt`) instantiate that view,
      so the when members (fields, constraints including select
      distributions, methods, on blocks) apply for those instances;
      `is a` checks walk the view's base chain. Chained modifiers and
      nested when-in-when blocks compose later; base-typed instances
      do not dynamically match subtypes yet; there is no type checking
- [~] Constraint IR for `keep` expressions — started: `src/solve.rs`
      translates conjunctions of linear comparisons over bounded integer
      fields into a solver problem; range membership (`x in [lo..hi]`)
      translates to the two comparisons; unsupported shapes fall back to
      rejection sampling. No disjunctions (item-list membership, `||`),
      negation trees or global constraints yet. `keep bind` and
      `hdl_path()` / `delayed()` constraints do not gate `gen`.
      Connected field sets (10.2.7) scope generation, v1: `gen <obj>`
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
      (clause 20.2.2 — the body at least once), `break`/`continue`,
      `return`, `print`/`out`/`outf`/`message`, `check that` with a
      DUT-error count, `pack`, `append`, lists with `size()`/`push()`,
      struct instances, sequence `do` (generates the item in place; the
      `keeping` block is not enforced), and `gen`. The optional `do`
      sugar on `if`/`while` — including the one-statement
      `if cond do <statement>;` form — parses and runs. Entry point is
      the built-in `sys` unit's `run()` methods, and implicit
      generation plus
      a MAIN sequence start also run when no explicit `run()` exists.
      Integers are unbounded `i128` (widths are not truncated). No
      event-driven scheduling
- [~] Events, temporal expressions and time-consuming methods — started:
      event triggers are stored, `emit` records a firing and executes the
      event's `on` action blocks inline, in composition order (clause
      11.2; each block with a fresh locals frame on the owning
      instance), and `compute` calls a TCM synchronously. The clause
      13.1.1 scheduler runs concurrent TCMs: `start` enqueues a
      coroutine with the implicit sync cycle before it commences, and
      coroutines take round-robin turns that run atomically from one
      synchronization action to the next — `wait`/`sync` suspend (with
      the captured locals frame and a skip-cursor that re-enters `while`
      chains mid-iteration), and a cycle passes when nothing is
      eligible. The clause 12 temporal-engine core
      (`src/temporal.rs`) is driven by the run loop's event log: `emit`
      records occurrences in per-cycle states, temporal `wait`s
      (`@event`, sequences, propositions, event repeats, with 13.1.3's
      readability-only `until`) re-evaluate at every turn against the
      grown log, a temporal wait unsatisfied for 1,000 cycles (or the
      5,000-cycle shared budget) reports `E0409`, and
      `expect`/`assume` rules (elaborated per type) are checked at quit
      per 12.3.2 — a failed rule runs its `else` body (or prints the
      failure when there is none) as a check error. One global
      timeline, and propositions evaluate with current field values
      (no per-cycle value history). Suspension re-enters `while` chains
      only — waits inside `if`/`case`/`for` bodies, inline calls,
      `compute` and `on` blocks keep the inline spin — and `wait` does
      not advance the HDL simulator
- [~] Coverage data model and reports — started: `src/coverage.rs`
      builds buckets and grades from the recorded groups (14.2's
      ranges/defaults, 14.5 merging), the interpreter samples on the
      group's event, derived items collect (14.4 transitions as
      previous→current pairs, 14.3 crosses over the sources' bucket
      labels), `per_instance = TRUE` grades per bucket with holes
      (14.2.1 — the group grade is the weighted instance grades plus
      the per_type bucket), `at_least`/`weight` apply from the option
      tokens, and `run --coverage` reports instances; coverage
      configuration (28.9), per-unit-instance options (14.1.1) and
      the remaining option tables are not collected
- [ ] Coverage configuration options (28.9) and per-unit-instance options (14.1.1)
- [~] Part 2 predefined routine library — started with the reflection
      set: `set_of_values(<type>)`/`full_set_of_values(<type>)`
      (5.8.4/5.8.5 — the argument parses as a type) build the
      `Value::Set` of all legal values (5.1.10 canonical intervals:
      sorted, merged, non-neighboring), the 27.4 set pseudo-methods
      query and combine (`size`/`uint_size`/`min`/`uint_min`/
      `int_min`/`max`/`uint_max`/`int_max`, the `_is_uint` guards,
      `union`/`intersect`/`diff`), `to_string()` renders scalars and
      sets, and `as_a` to an enumeral type maps the integer index to
      the enumerand (5.8.1). Set literals in expressions, range
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
      simulator is loaded). This is one simulator binding, not a
      multi-backend abstract API, and there is no VHPI
- [~] SystemVerilog integration tests and event ordering — started: the
      compose `mixed` service runs `examples/mixed_counter.e` against
      `examples/mixed/counter.sv`. Event ordering between e and the HDL
      simulator is not implemented
- [ ] WASM build and browser sandbox API

All stages require standards-aware conformance testing; compatibility with commercial products is not assumed.
