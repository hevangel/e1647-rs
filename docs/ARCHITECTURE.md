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

## Initial implementation

`src/lexer.rs` contains the current tokenizer. It intentionally leaves keyword recognition, full literal syntax, and many e-specific lexical rules to later milestones. `src/main.rs` exposes only `e1647 tokens`.
