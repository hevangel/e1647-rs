# Roadmap

## M0 — Repository and infrastructure
- [x] Rust package, README, license, tests, CI definition
- [x] First tokenization smoke tests
- [ ] Verify CI and tests on GitHub (repository must be published first)

## M1 — Language frontend
- [ ] Expand lexical grammar with golden IEEE 1647-based fixtures
- [ ] Build AST and parser
- [ ] Add symbol resolution, types, diagnostics
- [ ] Differential corpus against an authorized reference simulator where available

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
