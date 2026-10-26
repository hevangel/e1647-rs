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
      pinned under `external/` (not yet the complete lexical grammar:
      character literals, `<' .. '>` code literals and remaining operator
      spellings are open)
- [x] Build AST and parser for a declared subset: `struct`/`unit`/`extend`/
      `type` with `like`, fields (including name groups), scalar/list/named
      types and enumeral types, `keep`/`keep soft` constraints with C-style
      precedence, `when` subtype members (`value'id` modifiers), methods with
      parameter lists and statement bodies (local `var`, assignment, calls,
      indexing, `return`, braced `if`/`else`, `while` with `break`/
      `continue`, `print`, `gen`), spanned diagnostics and declaration/
      member/statement-level recovery (not yet: `when` elaboration,
      `for`/`repeat` loops, single-statement `if ... do`, temporal
      expressions, coverage)
- [~] Add symbol resolution, types, diagnostics — minimal single-file
      well-formedness checks only (duplicate declaration/field names,
      duplicate parameters/local variables/enumerals, unknown named/`like`/
      `extend` types now including method signatures and `list of` elements,
      unknown `keep` identifiers checked against fields and enumerals, zero
      widths); no type system or cross-file resolution yet
- [ ] Differential corpus against an authorized reference simulator where
      available (started: Specman reference runs via EDA Playground are
      recorded in docs/VERIFICATION.md; no automated differential harness yet)

## M2 — Elaboration and randomization
- [~] `extend`, `when`, units, structs and inheritance — elaboration
      started: `like` chains and `extend` blocks fold into one composed
      member list per type (`src/elab.rs`, `e1647 elab`), with
      inheritance-cycle, cross-declaration duplicate-field and
      unmodified-method-redefinition diagnostics; `when` subtypes are
      carried, not resolved, and there are no semantics yet
- [ ] Constraint IR for `keep` expressions
- [~] Reproducible seeded randomized objects — started: `gen` is a seeded
      rejection sampler over top-level `keep` constraints
      (`src/interp.rs`, `e1647 run --seed`); not a solver — no SMT/pluggable
      solver yet, soft and `when` constraints are not enforced
- [ ] Pluggable SMT/constraint solver
- [ ] Reproducible seeded randomized objects

## M3 — Behavioral execution
- [~] Interpreter and method calling conventions — started: the
      non-temporal statement subset executes (`src/interp.rs`): expressions,
      `var`, assignment, calls, `if`/`while`/`break`/`continue`, `return`,
      `print`/`out`, lists with `size()`/`push()`, struct instances, `gen`;
      entry point is the built-in `sys` unit's `run()` methods. No
      events/temporal semantics, no time-consuming methods, no width
      truncation of integer arithmetic
- [ ] Events, temporal expressions and time-consuming methods
- [ ] Coverage data model and reports

## M4 — Simulator integration
- [ ] Abstract HDL bridge API
- [ ] SystemVerilog integration tests and event ordering
- [ ] WASM build and browser sandbox API

All stages require standards-aware conformance testing; compatibility with commercial products is not assumed.
