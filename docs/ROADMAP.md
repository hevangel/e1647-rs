# Roadmap

## M0 — Repository and infrastructure
- [x] Rust package, README, license, tests, CI definition
- [x] First tokenization smoke tests
- [ ] Verify CI and tests on GitHub (repository must be published first)

## M1 — Language frontend
- [x] Expand lexical grammar: keyword recognition, e operators, sized/unsized
      based numerals, `0x`/`0b` literals and reals, with golden fixtures and a
      set-equality cross-check against the MIT-licensed reference grammar
      pinned under `external/` (not yet the complete lexical grammar:
      character literals, `<' .. '>` code literals and remaining operator
      spellings are open)
- [x] Build AST and parser for a declared subset: `struct`/`unit`/`extend`
      with `like`, fields, scalar/list/named types, `keep` constraint
      expressions with C-style precedence, spanned diagnostics and
      declaration-level recovery (not yet: methods, `when`, statements,
      temporal expressions, coverage)
- [~] Add symbol resolution, types, diagnostics — minimal single-file
      well-formedness checks only (duplicate declaration/field names, unknown
      named/`like`/`extend` types, unknown `keep` field roots, zero widths);
      no type system or cross-file resolution yet
- [ ] Differential corpus against an authorized reference simulator where
      available (started: Specman reference runs via EDA Playground are
      recorded in docs/VERIFICATION.md; no automated differential harness yet)

## M2 — Elaboration and randomization
- [ ] `extend`, `when`, units, structs and inheritance
- [ ] Constraint IR for `keep` expressions
- [ ] Pluggable SMT/constraint solver
- [ ] Reproducible seeded randomized objects

## M3 — Behavioral execution
- [ ] Interpreter and method calling conventions
- [ ] Events, temporal expressions and time-consuming methods
- [ ] Coverage data model and reports

## M4 — Simulator integration
- [ ] Abstract HDL bridge API
- [ ] SystemVerilog integration tests and event ordering
- [ ] WASM build and browser sandbox API

All stages require standards-aware conformance testing; compatibility with commercial products is not assumed.
