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
      (`list (key:it) of t`) are recorded in type positions;
      instantiation (monomorphization, clause 8.3) is a later milestone,
      and `new [<type>] [with { .. }]` parses but evaluates to an
      explicit not-supported diagnostic. Coverage groups (clause 14.1)
      parse and are recorded, not collected: `cover <event> is [also]
      { item/cross/transition entries; option entries }` and `cover
      <event> is empty;` — item types and sampled expressions are
      structural, every `using` option clause and group option entry is
      recorded verbatim; the coverage data model is a later milestone.
      Predefined non-scalar types `file` and `event` are accepted in
      type positions; `for each <type> (v) in list` records the element
      type filter. `expect`/`assume` members (clause 11.4) parse their
      temporal expressions into the clause 12 grammar
      (`TemporalExpr`: `@event` atoms, `cycle`, `true(exp)`,
      `change/rise/fall(exp)` edge samples, `{;}` sequences, `=>`
      yield, `and`/`or`/`not`/`fail`/`eventually`, `[n]*`/`~[l..h]*`/
      `[l..h]*` repeats, and the `te @event` sampling operator;
      `detach`/`delay`/`exec` report `E0120`) with the `else` failure
      body parsed as ordinary actions — sampling propagation (12.1.4)
      and evaluation over paths (12.3) are the temporal engine, a later
      milestone. `select { weight : choice; .. }` distribution
      expressions (clause 10) parse — weights structurally, choices
      verbatim — and the rejection sampler treats select constraints as
      no-ops until the distribution arc.
      Preprocessor directives (clause 21): `#ifdef` / `#ifndef` /
      `#define` / `#undef` in statement, struct-member and action
      positions with flag semantics — `#define` records the flag but
      applies no text substitution, and `#else` is not parsed. Spanned
      diagnostics and declaration/member/statement-level recovery. The
      CLI resolves `import a.b` to `a/b.e` or `a.b.e` beside the source
      and checks the merged declarations (not yet: `when` elaboration,
      `repeat`, single-statement `if ... do`, template instantiation
      (clause 8.3), template `extend` (clause 8.2), macro
      match-expression interpretation and expansion
      (clause 15.4–15.5), the coverage data model and reports
      (clause 14 options, buckets, grades), the clause 12 temporal
      engine — sampling propagation and evaluation over paths,
      `detach`/`delay`/`exec` TEs, select distribution
      biasing in the sampler, `$1`-`$27` match captures, `#define`
      replacement rules, `#else`)
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
      extends in declaration order.
      `when` subtypes are carried, not resolved; there is no type checking
- [~] Constraint IR for `keep` expressions — started: `src/solve.rs`
      translates conjunctions of linear comparisons over bounded integer
      fields into a solver problem; range membership (`x in [lo..hi]`)
      translates to the two comparisons; unsupported shapes fall back to
      rejection sampling. No disjunctions (item-list membership, `||`),
      negation trees or global constraints yet. `keep bind` and
      `hdl_path()` / `delayed()` constraints do not gate `gen`
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
      assignment, calls, `if`/`while`/`for`/`case`, `break`/`continue`,
      `return`, `print`/`out`/`outf`/`message`, `check that` with a
      DUT-error count, `pack`, `append`, lists with `size()`/`push()`,
      struct instances, sequence `do` (generates the item in place; the
      `keeping` block is not enforced), and `gen`. Entry point is the
      built-in `sys` unit's `run()` methods, and implicit generation plus
      a MAIN sequence start also run when no explicit `run()` exists.
      Integers are unbounded `i128` (widths are not truncated). No
      event-driven scheduling
- [~] Events, temporal expressions and time-consuming methods — started
      as syntax plus an inline approximation: event triggers are stored,
      `emit` records a firing and executes the event's `on` action
      blocks inline, in composition order (clause 11.2; each block with a
      fresh locals frame on the owning instance), `start` calls the
      method synchronously,
      and `wait` advances a virtual cycle counter under a fixed budget
      (`E0409`). No `@sim` sampling, no TCM suspension, and `wait` does
      not advance the HDL simulator
- [ ] Coverage data model and reports

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
