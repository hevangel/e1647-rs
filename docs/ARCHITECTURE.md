# Architecture (proposed)

```text
.e source -> lexer -> parser -> AST -> elaborator -> typed IR
                                                  |
                                      constraint / coverage / temporal IR
                                                  |
                                       interpreter + event scheduler
                                                  |
                                       HDL adapter (future xezim/VHPI)
```

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
  comments. Reports the first lexical error; later stages collect many.
- `src/ast.rs` — typed AST for declarations (including `type` enumerals and
  `when` subtypes), fields, type expressions, constraint expressions,
  methods and statements, all carrying byte/line/column spans.
- `src/parser.rs` — recursive-descent parser for the declared subset with
  C-style operator precedence and error recovery at declaration, member and
  statement boundaries. Diagnostics are collected (codes `E01xx` syntax,
  `E02xx` well-formedness; see the module documentation) and the AST stays
  partial under errors.
- `src/check.rs` — minimal single-file well-formedness checks (duplicate
  declaration/field/parameter/local/enumeral names, unknown type names,
  `keep` identifier roots, zero widths). **Not** a symbol/type checker yet:
  no type system, no cross-file scope, no elaboration.
- `src/elab.rs` — member composition: `like` chains and `extend` blocks fold
  into one composed member list per `struct`/`unit` (inheritance-cycle,
  cross-declaration duplicate-field and unmodified-method-redefinition
  diagnostics; codes `E03xx`). **Not** full elaboration: `when` subtypes
  are carried, not resolved; no semantics.
- `src/builtins.rs` — the predeclared environment names the executed subset
  relies on: the `sys` unit and the `out` routine.
- `src/interp.rs` — interpreter for the non-temporal statement subset with
  a seeded `gen` (codes `E0401`–`E0407`). Entry: the built-in `sys` unit's
  `run()` methods. Integers are unbounded `i128` (widths are not
  truncated). `gen` uses the constraint-solver backend when that compiles
  in, and rejection sampling otherwise.
- `src/solve.rs` — constraint IR and the pluggable solver seam: the
  top-level `keep` constraints of one type translate (conservatively) into
  bounded-integer variables plus a conjunction of linear comparisons.
  Backend: Pumpkin (pure-Rust CP solver) behind the `solver-pumpkin`
  cargo feature — seeded, deterministic; without the feature `solve`
  reports unsupported and `gen` falls back to rejection sampling.
- `src/main.rs` — `e1647 tokens`, `e1647 parse`, `e1647 elab` and
  `e1647 run [--seed N]` CLI commands; `parse`, `elab` and `run` print
  diagnostics to stderr (`path:line:col: error[code]: message`) and their
  results on success.

Deliberately not implemented yet: `when`-subtype resolution, non-linear,
disjunctive or temporal constraint solving (the solver backend covers
conjunctive linear constraints over bounded integers), events/temporal,
coverage, integer width truncation, character literals, and any HDL bridge.
