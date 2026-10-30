# Architecture (proposed)

```text
.e source -> lexer -> parser -> AST -> elaborator (member composition)
                                                  |
                          constraint IR (conjunctive linear keep) + interpreter
                                                  |
                          xezim VPI bridge (line protocol; one simulator)
```

Still ahead of the code: a typed IR, coverage, an event scheduler, a
multi-simulator HDL API, and wasm32.

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
  in the first column with no other text on that line (clause 4.1.2);
  prose outside those segments is skipped. A file with no begin marker
  is tokenized as code throughout. Decimal integers take a `K`/`M`
  multiplier, `0c"..."` is a character literal, and non-decimal based
  literals accept MVL digits. A bare `'` is punctuation, so
  `'path.to.signal'` can be parsed later. Reports the first lexical
  error; later stages collect many. MVL literals are tokenized and not
  evaluated. Block comments (`/* */`) are still accepted beyond
  clause 4.1.3.
- `src/ast.rs` — typed AST for declarations (including `type` enumerals,
  `when` subtypes, `simple_port`, `event`, `import` and `define`), fields,
  type expressions, constraint expressions, methods (including a TCM
  `@event` suffix) and statements, all carrying byte/line/column spans.
- `src/parser.rs` — recursive-descent parser for the declared subset.
  Expression precedence follows clause 4.5: C order for the arithmetic
  and boolean operators, with `^` looser than `|`, `===` looser than
  `==`, and `in` between `===` and `&`. Error recovery is at declaration,
  member and statement boundaries. The statement subset includes braced `if`/`while`,
  `for each` and `for i from .. to`, `case`, `check that`, `wait`, `emit`,
  `start`, sequence `do` and `stop_run`. Diagnostics are collected (codes
  `E01xx` syntax, `E02xx` well-formedness; see the module documentation)
  and the AST stays partial under errors.
- `src/check.rs` — minimal well-formedness checks (duplicate
  declaration/field/parameter/local/enumeral names, unknown type names,
  `keep` identifier roots, zero widths). The CLI runs this once on the
  import-merged declaration list. **Not** a symbol/type checker: no type
  system and no subtype-aware scope.
- `src/elab.rs` — member composition: `like` chains and `extend` blocks fold
  into one composed member list per `struct`/`unit` (inheritance-cycle,
  cross-declaration duplicate-field and unmodified-method-redefinition
  diagnostics; codes `E03xx`). Also records `simple_port` HDL paths and
  recovered `sequence` declarations. **Not** full elaboration: `when`
  subtypes are carried, not resolved; no type checking.
- `src/builtins.rs` — predeclared names the executed subset relies on:
  the `sys` unit, sequence bases (`any_sequence_item` and siblings), and
  routines such as `out`, `outf`, `message`, `pack`, `append`,
  `dut_error`, `stop_run`, the objection no-ops, and the `hdl_*` bridge
  calls.
- `src/interp.rs` — interpreter for the statement subset (codes
  `E0401`–`E0409`). Entry: the built-in `sys` unit's `run()` methods, with
  implicit field generation before `run()`. Integers are unbounded `i128`
  (widths are not truncated). `gen` uses the constraint-solver backend
  when that compiles in, and rejection sampling otherwise. `start` runs
  the called method inline; `wait` advances a virtual cycle counter and
  does not drive the simulator; `emit` records a firing and does not
  schedule anything.
- `src/solve.rs` — constraint IR and the pluggable solver seam: the
  top-level `keep` constraints of one type translate (conservatively) into
  bounded-integer variables plus a conjunction of linear comparisons.
  Backend: Pumpkin (pure-Rust CP solver) behind the `solver-pumpkin`
  cargo feature — seeded, deterministic; without the feature `solve`
  reports unsupported and `gen` falls back to rejection sampling.
- `src/hdl.rs` — one SystemVerilog binding. `HdlSession` spawns `xezim`
  with `c/xezim_bridge.c` and exchanges `SET` / `GET` / `RUN` / `FINISH`
  lines. Arguments are allowlisted; there is no shell. Quoted HDL paths
  and the `hdl_*` routines use this session.
- `src/main.rs` — `e1647 tokens`, `e1647 parse`, `e1647 elab` and
  `e1647 run [--seed N]`. `import a.b` loads `a/b.e` or `a.b.e` next to
  the source and splices those declarations in front. `parse`, `elab` and
  `run` print diagnostics to stderr
  (`path:line:col: error[code]: message`) and their results on success.

Deliberately not implemented yet: `when`-subtype resolution, non-linear,
disjunctive or temporal constraint solving (the solver backend covers
conjunctive linear constraints over bounded integers), an event scheduler
and time-consuming-method suspension, coverage, integer width truncation,
evaluation of MVL literals, `repeat`, single-statement `if ... do`, `on`
blocks, `define as` templates, a multi-simulator HDL API, and WASM.
