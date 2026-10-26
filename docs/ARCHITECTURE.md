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
- `src/ast.rs` — typed AST for declarations, fields, type expressions and
  constraint expressions, all carrying byte/line/column spans.
- `src/parser.rs` — recursive-descent parser for the declared subset with
  C-style operator precedence and declaration/member-level error recovery.
  Diagnostics are collected (codes `E01xx` syntax, `E02xx` well-formedness;
  see the module documentation) and the AST stays partial under errors.
- `src/check.rs` — minimal single-file well-formedness checks. **Not** a
  symbol/type checker yet: no type system, no cross-file scope, no
  elaboration.
- `src/main.rs` — `e1647 tokens` and `e1647 parse` CLI commands; `parse`
  prints diagnostics to stderr (`path:line:col: error[code]: message`) and
  the debug AST on success.

Deliberately not implemented yet: elaboration (`extend`/`when` composition),
constraint solving, randomization, events/temporal, coverage, execution and
any HDL bridge.
