# Verification log

Append-only log of verification work: exact commands, results, scope and
known limitations. A successful build alone does not verify acceptance; every
claim here must name the command that backs it.

## 2026-10-25 — issue #1 staged front-end work (branch `agent/issue-1-foundation`)

Scope claimed for this change (nothing broader):

- Tokenizer: keyword recognition, multi-character operators, sized/unsized
  numeral and bit-literal handling, same token stream contract as before.
- Parser: recursive-descent parser for the declared subset (struct / unit /
  extend declarations, field declarations, scalar and list types, `keep`
  constraint expressions, `like` inheritance clause) with spanned
  diagnostics and error recovery at declaration level.
- Minimal well-formedness checks: duplicate field names, unknown named types,
  duplicate declaration names. **This is not full symbol/type checking.**
- CLI: `e1647 parse <file>` command with diagnostic output and exit codes.
- Reference-parsing support: EDA Playground credentials handling (`.env`),
  reference repository pins under `external/`, governance documents.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`, run locally on
Windows; identical commands run in CI on ubuntu-latest via `.github/workflows/ci.yml`):

```
$ cargo fmt --all -- --check
(no output — pass)

$ cargo clippy --all-targets -- -D warnings
    Checking e1647-rs v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.05s
(pass, zero warnings)

$ cargo test --all-targets
test result: ok. 0 passed;  0 failed  (unittests src/lib.rs)
test result: ok. 0 passed;  0 failed  (unittests src/main.rs)
test result: ok. 1 passed;  0 failed  (test fixtures.rs — golden diagnostics)
test result: ok. 12 passed; 0 failed  (test lexer.rs)
test result: ok. 10 passed; 0 failed  (test parser.rs)
```

Manual CLI checks run additionally (not part of CI):

```
$ cargo run -- parse examples/packet.e        # prints the AST, exit 0
$ cargo run -- parse /tmp/bad.e               # duplicate field fixture
/tmp/bad.e:3:3: error[E0202]: duplicate field 'a'   (exit 1)
```

Known limitations after this change (still true unless a later entry says
otherwise):

- No type checking beyond the duplicate/unknown-name checks above; no `when`,
  no method declarations, no temporal expressions, no events, no coverage.
- Tokenizer/parse results are validated against our own fixtures plus
  cross-checks of the pinned MIT-licensed grammars, not against the IEEE
  1647 text (lawful personal copy kept locally under `local/`, not committed).
- Specman reference-run evidence via EDA Playground is recorded separately
  below when performed; absence of an entry means it has not been done.

## 2026-10-26 — methods, statements and enum types (branch `agent/issue-1-methods-types`)

Scope claimed for this change (nothing broader):

- Parser additions: `type` enumeral declarations (`type color_e: [RED,
  GREEN];`, optional `= value` per enumeral); method members with parameter
  lists and optional return types (`f(a: uint): uint is { .. };`, with
  `is also` / `is first` / `is only`); statement bodies (local `var`
  declarations with optional initializer, assignment, expression statements,
  `return`, braced `if`/`else`, `print`, `gen`); call `f(x)` and index
  `a[i]` postfix expressions; `keep soft` constraints; field name groups
  (`a, b: uint;` declares one field per name). Lowercase `true`/`false`
  now parse as boolean literals. The former E0114 rejections (method
  calls, `soft`, name groups) are gone — `E0114` is reserved, `E0115`
  (expected `is` in a method) is new, `E0109` is now the `(` expectation.
- Checks additions: duplicate parameters and local variables share one
  scope per method (E0206); duplicate enumerals within a `type` (E0206);
  unknown named types now also reported in method parameter/return/local
  types and inside `list of` elements (E0203); `keep` constraint roots
  accept declared enumerals and `me`/`self`, and call callees no longer
  count as field roots (E0204 message is now "unknown identifier").
- AST: `MethodDecl`, `Param`, `MethodModifier`, `Enumeral`, `Stmt`;
  `Expr` gained `Call` and `Index`; `KeepConstraint` gained `is_soft`.
- Examples: `examples/methods_types.e` (parses clean).
- 5 new golden diagnostic fixtures: duplicate_local, duplicate_param,
  unknown_method_type, duplicate_enumeral, keep_unknown_enumeral (16
  fixture pairs total).

Not in this change: `when` declarations, loops (`while`/`for each` are
rejected with E0113), the single-statement `if ... do ...` form, method
block scoping (parameters and locals are one flat scope per method), and
any semantics — statements are parsed, not interpreted.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`, run locally
on Windows; identical commands run in CI on ubuntu-latest via
`.github/workflows/ci.yml`):

```
$ cargo fmt --all -- --check
(no output — pass)

$ cargo clippy --all-targets -- -D warnings
    Checking e1647-rs v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.29s
(pass, zero warnings)

$ cargo test --all-targets
test result: ok. 0 passed;  0 failed  (unittests src/lib.rs)
test result: ok. 0 passed;  0 failed  (unittests src/main.rs)
test result: ok. 1 passed;  0 failed  (test fixtures.rs — 16 golden pairs)
test result: ok. 12 passed; 0 failed  (test lexer.rs)
test result: ok. 16 passed; 0 failed  (test parser.rs)

$ cargo run -- parse examples/methods_types.e    # prints the AST, exit 0
$ cargo run -- parse examples/reference_subset.e # exit 1 — `extend sys`
  is not declared in that file (E0203); pre-existing behavior, unchanged
```

## 2026-10-26 — when subtypes and while loops (same branch, second increment)

Scope claimed for this change (nothing broader):

- Lexer: a bare `'` followed by an identifier is now a standalone
  punctuation token (the `read'kind` subtype modifier quote), matching the
  reference grammar's `SNG_QUOTE`. Base letters after `'` still start based
  literals. The invalid_lex fixture and the lex-error unit test switched
  from `'q1`/`'z1` to `''` (a quote followed by an identifier is no longer
  a lexical error).
- Parser: `when` subtype members — one or more `value'id` / bare `id`
  modifiers per the reference grammar's `struct_type_modifier` (the base
  struct name, when present, is the trailing bare-id modifier), then a
  member body — and `while` loops with `break`/`continue` statements.
  Member lists are shared between declaration bodies and `when` bodies.
- Checks: fields, methods and constraints are collected through `when`
  members. Duplicate fields are diagnosed across the whole declaration
  including `when` blocks; a `keep` inside a `when` may reference any field
  of the declaration. Subtype-aware scoping is elaboration and stays
  undone (documented in check.rs).
- 2 new golden fixtures (when_bad_quote, when_missing_modifier — the
  latter deliberately documents the recovery cascade into E0101), 4 new
  parser tests, 1 new lexer test. Totals: 18 fixture pairs, 13 lexer tests,
  20 parser tests.

Not in this change: `when` elaboration semantics, `for`/`repeat` loops,
`when` in covergroups, single-statement `if ... do`, and any semantics —
statements are parsed, not interpreted.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`, run locally
on Windows; identical commands run in CI on ubuntu-latest via
`.github/workflows/ci.yml`):

```
$ cargo fmt --all -- --check
(no output — pass)

$ cargo clippy --all-targets -- -D warnings
    Checking e1647-rs v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.92s
(pass, zero warnings)

$ cargo test --all-targets
test result: ok. 0 passed;  0 failed  (unittests src/lib.rs)
test result: ok. 0 passed;  0 failed  (unittests src/main.rs)
test result: ok. 1 passed;  0 failed  (test fixtures.rs — 18 golden pairs)
test result: ok. 13 passed; 0 failed  (test lexer.rs)
test result: ok. 20 passed; 0 failed  (test parser.rs)

$ cargo run -- parse examples/methods_types.e    # prints the AST, exit 0
```

## 2026-10-26 — like/extend elaboration (same branch, third increment)

Scope claimed for this change (nothing broader):

- New `src/elab.rs`: `like` chains and `extend` blocks fold into one
  composed member list per `struct`/`unit` (base-most `like` members
  first, then the declaring members, then each `extend` block in source
  order; order-independent on declarations — an `extend` may precede its
  target). `type` declarations pass through with their enumerals; `when`
  members are carried, ancestor types first, not resolved.
- New diagnostics: `E0301` like inheritance cycle (once per type), `E0302`
  field name contributed by two different declarations/extend blocks (once
  per name; within one declaration remains the parser's `E0202`), `E0303`
  method redefined without `is also`/`is first`/`is only` (once per name).
- New library API `e1647::elab::elaborate(&[Declaration]) -> Elaborated`;
  parsing (`parse()`) is unchanged and elaboration does not run inside it.
- CLI: new `e1647 elab <file>` command printing the elaborated types
  (enumerals, fields with rendered types, constraints, method signatures,
  `when` blocks) or diagnostics; exit 1 on any diagnostic.
- 9 new tests in `tests/elab.rs` (composition order, forward extends,
  carried `when` members, cycles, duplicate fields across extends/like,
  method redefinition, type pass-through).

Not in this change: `when`-subtype resolution, type checking, constraint
IR, and any semantics.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`, run locally
on Windows; identical commands run in CI on ubuntu-latest via
`.github/workflows/ci.yml`):

```
$ cargo fmt --all -- --check
(no output — pass)

$ cargo clippy --all-targets -- -D warnings
    Checking e1647-rs v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.82s
(pass, zero warnings)

$ cargo test --all-targets
test result: ok. 0 passed;  0 failed  (unittests src/lib.rs)
test result: ok. 0 passed;  0 failed  (unittests src/main.rs)
test result: ok. 9 passed;  0 failed  (test elab.rs)
test result: ok. 1 passed;  0 failed  (test fixtures.rs — 18 golden pairs)
test result: ok. 13 passed; 0 failed  (test lexer.rs)
test result: ok. 20 passed; 0 failed  (test parser.rs)

$ cargo run -- elab examples/methods_types.e   # prints the composed types, exit 0
```

## 2026-10-26 — execution: interpreter, builtins, gen (same branch, fourth increment)

Scope claimed for this change (nothing broader):

- Lexer: the e code-block delimiters are recognized and skipped as trivia —
  `<'` opens and `'>` closes (the closing delimiter is quote-then-`>`; the
  first implementation attempt had the pair reversed and was caught by the
  new lexer test before any commit). Their contents are ordinary code.
- Builtins (`src/builtins.rs`): the predeclared `sys` unit and `out`
  routine. `extend sys` no longer reports E0203; the elaborator synthesizes
  a `sys` type from its extend blocks.
- Parser fix: number-token classification no longer misreads `0xE5` (hex
  `E`) as a real literal — `looks_like_real` requires a fraction or a
  decimal-digits-then-exponent shape.
- Interpreter (`src/interp.rs`, codes `E0401`–`E0407`): value model
  (unbounded integers — widths are NOT truncated, documented), expression
  evaluation including based literals and short-circuit logic, statement
  execution (`var`, assignment including list slots, `if`/`while`/
  `break`/`continue`, `return`, `print`), method calls (all composed blocks
  run in order; last returned value wins), `list of` with `size()`/`push()`,
  struct instances, and `gen` — a seeded rejection sampler over top-level
  `keep` constraints (soft/`when` constraints not enforced; not a solver;
  1000 attempts then E0407). Entry point: `sys.run()`.
- CLI: `e1647 run [--seed N] <file>`; parse and elaboration diagnostics
  fail the run before execution.
- Examples are now runnable programs: `methods_types.e` gained an
  `extend sys run()` entry point (and `frame_s` a `keep pixels.size() > 0`
  so `pick` is safe after gen); `reference_subset.e` runs unchanged;
  `packet.e` runs with no output (declarations only — sys has no `run()`).
- Tests: 12 new interpreter tests (tests/interp.rs) incl. determinism and
  E0407; 1 new elaborator test (builtin sys synthesis); 1 updated lexer
  test (code delimiters now skipped, not rejected). Totals: 10 elab, 1
  fixtures, 12 interp, 13 lexer, 20 parser = 56.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`, local
Windows; identical commands run in CI on ubuntu-latest via
`.github/workflows/ci.yml`):

```
$ cargo fmt --all -- --check
(no output — pass)

$ cargo clippy --all-targets -- -D warnings
    Checking e1647-rs v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.46s
(pass, zero warnings)

$ cargo test --all-targets
test result: ok. 0 passed;  0 failed  (unittests src/lib.rs)
test result: ok. 0 passed;  0 failed  (unittests src/main.rs)
test result: ok. 10 passed; 0 failed  (test elab.rs)
test result: ok. 1 passed;  0 failed  (test fixtures.rs — 18 golden pairs)
test result: ok. 12 passed; 0 failed  (test interp.rs)
test result: ok. 13 passed; 0 failed  (test lexer.rs)
test result: ok. 20 passed; 0 failed  (test parser.rs)

$ cargo run -- run examples/packet.e
(no output, exit 0 — declarations only, no sys.run())

$ cargo run -- run --seed 1 examples/reference_subset.e
p = packet_s{addr: 100, data: 132, tag: 12, par: [7]}
based literals: 8'hFF=255 0b1010=10 0o17=15e1647-rs subset constructs accepted by Specman
(exit 0 — gen satisfied keep addr < 128, keep addr > 0, keep data != 0)

$ cargo run -- run --seed 1 examples/methods_types.e
165
165
255
219
methods_types.e ran to completion
(exit 0 — gen pixel + frame (list element gen), method calls, the
if-clamp in scale() reduced 165*2=330 to 255)
```

## Reference-simulator runs (EDA Playground)

- **2026-10-25 — first attempt blocked.** Automated login to
  edaplayground.com with the `.env` account was blocked by reCAPTCHA
  Enterprise (`loginpage?error=captcha` twice). No CAPTCHA solving is
  performed by policy. Recorded in full in the PR #2 description.
- **2026-10-26 — example prepared and saved; run blocked by account
  validation.** The owner logged in manually in the automation browser.
  Independently authored example `examples/reference_subset.e` (struct with
  `uint(bits: 8)` / `nibble` / `list of` fields and `keep` constraints,
  `extend` with a constraint, `struct ... like`, `gen`/`print`, based
  literals `8'hFF`, `0b1010`, `0o17` in `out()`) was entered into the
  testbench editor with language **Specman e + SV/Verilog** and simulator
  **Cadence Xcelium 25.03** (Specman 2025.03), and saved at
  <https://www.edaplayground.com/x/7Jrh>. Clicking Run opened
  `validatedAccessRequiredModal`: **"Your account is not validated"** —
  commercial simulators require a validated account. The confirmation email
  resend (`/resendConfirmation`) was requested.
- **2026-10-27 — reference run succeeded (Specman 2025.03, Xcelium
  25.03).** New credentials were placed in `.env` (gitignored; never
  quoted here). Automated login via the built-in browser succeeded on the
  first attempt (form fill + reCAPTCHA coordinate click; no
  `error=captcha` redirect), and the previously validation-gated
  commercial flow ran without the `validatedAccessRequiredModal`.
  Run 1 failed with `xmelab: *F,NOTOP: A top level design unit was not
  precalculated …` — a pure-e playground needs at least a trivial SV top
  in the Design pane. After adding a minimal `module design; endmodule`,
  the run completed (exit 0). Tool log, trimmed to the relevant lines:

  ```
  xrun -Q -unbuffered '-timescale' '1ns/1ns' '-sysv' '-access' '+rw' design.sv testbench.e
  TOOL:	xrun	25.03-s001: Started on Oct 27, 2026 at 02:28:44 EDT
  Loading snapshot worklib.design:sv .................... Done
  Initializing Xcelium testbench (25.03 Build 426593)
  Checking license ... OK
  Doing setup ...
  Generating the test with IntelliGen using seed 1...
  Starting the test ...
  Running the test ...
    p = packet_s-@1: packet_s   of unit: sys
  0	addr:                           115
  1	data:                           137
  2	tag:                            14
  3	par:                            (24 items)
  based literals: 8'hFF=255 0b1010=10 0o17=15
  e1647-rs subset constructs accepted by Specman
  Checking the test ...
  Checking is complete - 0 DUT errors, 0 DUT warnings.
  xmsim: *W,RNQUIE: Simulation is complete.
  TOOL:	xrun	25.03-s001: Exiting on Oct 27, 2026 at 02:29:03 EDT  (total: 00:00:19)
  ```

  Conclusions for this project:
  - Specman (IntelliGen, seed 1) compiled and ran the example: `gen p`
    produced addr=115, data=137, tag=14, par=(24 items) — every `keep`
    constraint of the example holds (addr < 128, addr > 0, data != 0).
  - Based-literal spellings are now **Specman-confirmed**: `8'hFF`=255,
    `0b1010`=10, `0o17`=15. The lexer's `h`/`H` hex and `0o` octal support
    no longer rests on grammar cross-checks alone.
  - The saved playground now lives at
    <https://www.edaplayground.com/x/kvcS> (copy of `/x/7Jrh` under the new
    account, including the minimal `design.sv` top; the original `/x/7Jrh`
    remains with the previous account and still fails with NOTOP).
  - Not yet cross-checked against this run: the full keyword set, error
    recovery shapes, and the interpreter's sampling semantics (Specman's
    IntelliGen is a solver, our `gen` is a rejection sampler — values are
    not expected to match, only constraint satisfaction).

- **2026-10-27 (2) — deterministic output comparison: 16/16 lines
  identical.** New example `examples/reference_check.e`: a fully
  deterministic program (arithmetic and precedence, shifts, bitwise ops,
  sized/prefixed based literals, comparisons, logic, unary, `mod`,
  division, method calls with loops, `while`/`break`/`continue`, list
  `push`/`size`/index/index-assign, enumeral comparison, string escapes).
  Run on both implementations with the identical file:

  - e1647-rs: `cargo run -- run --seed 1 examples/reference_check.e`
    (exit 0).
  - Specman: playground <https://www.edaplayground.com/x/8Fjd>
    (Specman 2025.03, Xcelium 25.03, exit 0, 0 DUT errors).

  Both print the same 16 labeled lines, including both probes:

  ```
  arith=7 / paren=9 / shift=16 32 / bits=8 14 6
  lits=255 171 10 15 170      (8'hFF 0xAB 0b1010 0o17 8'b1010_1010)
  cmp=TRUE TRUE FALSE / logic=TRUE TRUE / unary=3 / mod=2
  int_div=3                   (probe: `/` on two ints is integer division)
  bitnot=-1                   (probe: `~0` is -1)
  call=42 21                  (sys methods with params/returns/while-loop)
  loop=1 3 4                  (continue skips 2, break stops at 5)
  list=2 42                   (push/size/index/index-assign)
  enum=TRUE / end=str-check-complete
  ```

  (The Specman log wraps lines between `out()` calls; the emitted text is
  the same.)

  Divergences found while building the comparison (both are e1647-rs
  being more permissive, not less):

  1. **Unsized based literals are rejected by Specman.**
     `'h10`, `'b1010`, `'o17` in expression position produce
     `*** Error: Unrecognized exp [Unrecognized expression ''h10']` on
     Specman 2025.03. e1647-rs lexes/parses/evaluates them (16/10/15).
     Kept as a documented superset; the comparison file uses only sized
     and `0x`/`0b`/`0o` prefixed literals.
  2. **Unallocated struct references crash Specman.** A bare
     `var m: math_s;` followed by a method call `m.add(...)` killed the
     simulator (`Exit code 137` after Specman crash-symbolization
     warnings) — real e requires the instance to be allocated (`gen` or
     `new`) first. e1647-rs auto-allocates a default instance for struct
     types. Follow-up: model `new` and null-reference semantics
     (roadmap); methods under test were moved onto `sys` so both
     implementations run the identical file.

  Also confirmed: e string escapes (`"\n"`) compile and behave as expected
  on Specman, and `out()` concatenates its arguments with no separator.

## 2026-10-27 (3) — constraint-solver backend (Pumpkin) behind a feature

Decision recorded (per the owner's direction to reuse existing solver
algorithms rather than hand-rolling):

| crate | verdict |
| --- | --- |
| `pumpkin-solver` 0.5 (consol-lab, MIT) | **chosen.** Pure-Rust lazy-clause-generation CP solver; bounded integers + linear (in)equalities map directly onto e randomization; API: `new_bounded_integer`, affine `scaled` views, `equals`/`less_than_or_equals` + negation, `satisfy`; `SolverOptions.random_generator` seeds determinism. |
| `z3` 0.21 (prove-rs, MIT) | capable SMT bindings, but require the native Z3 library — heavier dependency and CI story; kept as a future backend behind the same seam. |
| `oxiz` 0.3 (pure-Rust SMT) | young; revisit if pure-Rust SMT is needed (non-linear etc.). |
| `rustsat` 0.7 | SAT toolkit — would need our own order encoding; a CP/SMT API fits e constraints better. |
| `pcp` on crates.io | unrelated (Post Correspondence Problem searcher, 2016). |

Scope claimed for this change (nothing broader):

- New `src/solve.rs` (no dependencies): constraint IR (`Problem` =
  bounded integers + conjunction of linear comparisons), a conservative
  translator from the top-level `keep` constraints of one type over its
  integer scalar fields (bounds from the declared widths, capped at 31
  bits for the solver domain), and the backend seam. Unsupported shapes
  (non-integer fields, `||`, `!`, method calls, non-linear arithmetic)
  make the translation report unsupported.
- Pumpkin backend behind the `solver-pumpkin` cargo feature (default
  off): seeded `SolverOptions`, coefficients via affine `scaled` views,
  `!=` via constraint negation, `>=`/`>` via negated `≤` encodings;
  UNSAT reported as `SolveOutcome::Unsat`. `gen` prefers the solver and
  falls back to the rejection sampler on unsupported/unknown.
- `Interp` carries the run seed; `gen_object`/scalar `gen` gain the
  solver path (non-integer fields are still sampled).
- CI: new `check-solver` job running clippy + tests with
  `--all-features` on ubuntu. The local windows-gnu toolchain cannot
  build pumpkin (its build script needs a C compiler, and getrandom
  needs dlltool — neither exists locally), so the feature path is
  verified by CI, not locally; this is stated, not assumed.
- 6 always-on translation/seam tests (`tests/solve.rs`) + 4
  pumpkin-gated tests (UNSAT for `a+b==100 && b==2a`, deterministic
  solve of `a+b==100 && a>=40`, coefficient solve, UNSAT detection).

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`, local
Windows, default features — CI runs the same plus `--all-features` on
ubuntu-latest):

```
$ cargo fmt --all -- --check
(no output — pass)

$ cargo clippy --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.71s
(pass, zero warnings)

$ cargo test --all-targets
test result: ok. 6 passed;  0 failed  (test solve.rs — translation + seam)
test result: ok. 10 passed; 0 failed  (test elab.rs)
test result: ok. 1 passed;  0 failed  (test fixtures.rs — 18 golden pairs)
test result: ok. 12 passed; 0 failed  (test interp.rs)
test result: ok. 13 passed; 0 failed  (test lexer.rs)
test result: ok. 20 passed; 0 failed  (test parser.rs)

$ cargo test --all-targets --all-features   # CI (ubuntu) only: + 4 pumpkin tests
```

CI verification (the authoritative check for the feature path): the
`check-solver` job first failed on three of our API mistakes, each caught
and fixed in its own commit — a match on `Unknown` against a variant the
final enum no longer has, two missing trait imports
(`ProblemSolution`/`TransformableVariable`, then `NegatableConstraint`),
and a match whose arms mixed the distinct opaque `impl NegatableConstraint`
types returned by different builders (restructured to post per arm). The
  final run of commit `dcffd5c` is green: `cargo clippy --all-targets
  --all-features -- -D warnings` and `cargo test --all-targets
  --all-features` pass on ubuntu-latest, including the four pumpkin-gated
  tests (UNSAT for `a+b==100 && b==2*a`, deterministic solve of
  `a+b==100 && a>=40`, `2a-b==21` with coefficients, UNSAT detection for
  `a>100` on a 4-bit domain).

## 2026-10-27 (4) — containerized toolchain (Docker Compose)

`Dockerfile` + `compose.yml` + `.dockerignore` provide the full toolchain
as a container: `rust:1-slim` plus gcc/g++/pkg-config/git and the
rustfmt/clippy components, `CARGO_TARGET_DIR=/opt/target` (warm cache
outside the bind mount), and `cargo fetch --locked` at build time. The
compose services: `toolchain` (shell), `verify` (fmt + clippy + tests with
`--all-features`), `test` (default features), `demo` (run + elaborate the
examples). `.dockerignore` keeps `.env`, `local/`, `external/`, `target/`
and `.git` out of the build context — **no credentials enter the image**.

Two image-build findings, fixed in place:

1. pumpkin-solver's `build.rs` also compiles a **C++** proof checker
   (`maxsat-checker.cc`), so `g++` is required — gcc alone fails with a
   misleading `No such file or directory` from the cc crate.
2. The slim image ships the minimal Rust profile — `rustup component add
   rustfmt clippy` is required for the verification suite.

A placeholder-compile warm-up (building the dependency graph against dummy
sources at image build time) was deliberately removed: bind-mount mtime
skew made cargo reuse the stale placeholder-lib artifact instead of the
mounted sources.

Verification performed locally with Docker Desktop (linux containers):

```
$ docker compose build
Image e1647-rs-toolchain:latest Built

$ docker compose run --rm verify
cargo fmt --all -- --check                (pass)
cargo clippy --all-targets --all-features -- -D warnings   (pass, zero warnings)
cargo test --all-targets --all-features
test result: ok. 10 passed; 0 failed  (test solve.rs — incl. the 4
                                      pumpkin-gated tests)
(all other suites green; 66 tests total)

$ docker compose run --rm demo
165 / 165 / 255 / 219 / methods_types.e ran to completion
(elaborated types printed, incl. the synthesized `unit sys`)
```

$ docker compose run --rm verify   # fmt + clippy + tests, all features: green
$ cargo fmt / clippy / test (host, default features): green (66 tests)
```

## 2026-10-27 (6) — EDA Playground curated Specman-e examples: baseline

The four curated "Specman e + SV/Verilog" playgrounds (listed at
`/playgrounds?...&language=Specman+e+%2B+SV%2FVerilog&curated=true`) were
extracted into `local/eda-examples/` (gitignored — third-party code is not
committed) and measured against the front-end. **Result: 0 of 4 runnable
today, as expected for a pre-alpha subset** — all four are full
verification environments. What each one needs beyond the implemented
subset (from parse diagnostics plus source inspection):

| playground | diagnostics | first blockers / missing constructs |
| --- | --- | --- |
| `D6W` Hello e World | 37 | top-level `import`; `!` do-not-generate fields; `signal_map`/`simple_port` members; events (`is only cycle @p_smp.clock_f`); TCMs (`reset()@clock`), `wait [1]`, `emit`, `start`; `sequence ... using` + `extend MAIN`; objections |
| `Pfq` e XOR | 30 | port declarations (`in simple_port of bit is instance`), `keep bind(...)`/`hdl_path()`, `clk$` port reads, events (`fall(clk$) @sim`), TCM `verify() @xor_clk`, `for each in ... do`, `pack()`, `%`/`!` field modifiers, `check that ... else dut_error`, `start`, `stop_run()` |
| `5Wj` e SER | 15 | `package` + `import`, enumeral extension (`extend kind_t : [NEW]`), UVC env (`is a ACTIVE TX_agent`), `in [1..8]` constraints, `for i from 0 to n`, `wait [n] * cycle`, `specman("...")`, `set_config`, `pkg::Type` |
| `ZFqZ` UART agent | 15 | `import`, `!p_smp` fields, `@p_smp.clk` events, `when ACTIVE'active_passive agent` (when inheritance), TLM `port.connect()`, `on clk { emit ... }` |

This inventory is the concrete M2/M3 roadmap input (ports/signal maps,
events and TCMs, sequences, packages, `when` elaboration). One lexical gap
found and fixed in the same change: **e's native `--` line comments** now
lex (previously only `//` and `/* */`), with a lexer test. With that fix
the examples tokenize cleanly up to their first unsupported construct.

Regression after the lexer change: fmt/clippy/test all green (67 tests —
14 lexer tests now).

## 2026-10-27 (5) — mixed e + SystemVerilog testcases via xezim

Xezim (github.com/aionhw/xezim, the ecosystem's Rust SystemVerilog
simulator, pinned rev `058f2bba…` — the same baseline the
ai_hw_engineer toolchain image records) simulates the HDL side; the e
interpreter stays the master of verification and drives it through a VPI
control bridge — the same simulator-master architecture Specman uses for
simulator integration.

- `c/xezim_bridge.c`: VPI module. After elaboration it reads one command
  per line from stdin and answers exactly one line per command:
  `SET path value` (vpi_put_value, vpiNoDelay), `GET path` (vpiIntVal),
  `RUN delay` (cbAfterDelay; `READY` when the loop re-enters), `FINISH`
  (vpi_control(vpiFinish)).
- `src/hdl.rs`: `HdlSession` spawns `xezim --simulate --vpi-lib <bridge>
  -s <top> <file>` — the program name is fixed and every argument is
  validated against a character allowlist; the spawn is an argv list
  with no shell.
- Interp: new builtins `hdl_load` / `hdl_set` / `hdl_get` / `hdl_run` /
  `hdl_finish` (diagnostic `E0408` for bridge errors).
- `examples/mixed/counter.sv` + `examples/mixed_counter.e`: the e code
  resets an 8-bit counter DUT, clocks it five times and checks the count
  sequence — the Specman-style structure with an SV DUT.
- Docker: a `xezim-build` stage (pinned rev; autotools required by
  libffi-sys; built without the jit feature), the bridge compiled with cc
  against the xezim include directory; compose `mixed` service.

Findings along the way: the slim Rust image lacks git and the
rustfmt/clippy components (added); libffi-sys needs autotools
(build-essential + autoconf/automake/libtool added); the IEEE 1647
keyword `expect` cannot be a variable name in our own testcases; and the
first mixed run exposed an off-by-one in the testcase's expected
sequence (reads happen after each incrementing edge) — the bridge
itself worked from the first clean parse.

Verification performed (Docker Desktop, linux containers):

```
$ docker compose build        # image incl. xezim (pinned rev) + bridge.so
$ docker compose run --rm mixed
cycle 0: count=1 OK
cycle 1: count=2 OK
cycle 2: count=3 OK
cycle 3: count=4 OK
cycle 4: count=5 OK
mixed testcase complete

$ docker compose run --rm verify   # fmt + clippy + tests, all features: green
$ cargo fmt / clippy / test (host, default features): green (66 tests)
```

## 2026-10-27 (7) — real-example corpora baseline; quoted-path signal access

Two additional real-example corpora were inventoried per the owner's
direction, plus one language primitive landed to close the first gap.

Corpora:

- **asic-world Specman examples** (asic-world.com/examples/specman):
  memory / fifo / uart are multi-file projects (24 code blocks extracted
  into `local/asic-world/`, gitignored — third-party code is not
  committed); **arbiter is stub-only upstream** (Part I–III pages contain
  no code). These are old-school Specman testbenches: signal access via
  quoted paths (`'memory_tb.address' = object.addr;`), events
  (`event clk is rise('memory_tb.clk') @sim`), TCMs (`gen_cmds()@clk`),
  `outf("…%x…")`, ternaries, keyed lists (`list(key:addr) of …` with
  `.add/.key_exists/.key`), `start`, `wait cycle`.
- **uobdv/Design-Verification** — added as the submodule
  `external/design-verification` (read-only reference, pinned at
  `0a48307`). Its SN-CPU-Tutorial (`cpu/e/src`, 14 files: signal map,
  driver, refmodel, checker, coverage, tests) parse-probes at 1–29
  diagnostics per file; blockers are the same set: `simple_port`
  members (`inout simple_port of bit is instance`, `hdl_path()`,
  `delayed()`), events (`@sys.any`, `rise/fall('path') @sim`), TCMs
  (`clkgen() @sys.any`, `wait cycle`).

Gap inventory across all three corpora (EDA Playground curated +
asic-world + SN-CPU-Tutorial) now forms the M2/M3 roadmap input:
ports/signal maps, events, TCMs (+`start`/`wait`), sequences, packages,
`when` elaboration, keyed lists, `outf`/ternaries, `import`.

Landed in this change — **quoted-path signal access**, the core
primitive of the asic-world style, backed by the xezim bridge:

- Lexer: non-literal `'` quotes are standalone punctuation tokens (the
  closing quote of a path no longer raises a lexical error); character
  literals move from a lexical to a parse-level concern (fixtures
  `invalid_lex` and `when_bad_quote` updated; the based-literal error
  path is unchanged).
- AST/parser: `Expr::HdlPath` — `'path.to.signal'` parses as an
  expression and as an assignment target.
- Interp: reads and assignments route to the simulator session
  (`E0408` without `hdl_load`).
- `examples/mixed_counter.e` now drives the counter DUT in the
  asic-world style (`'counter.clk' = 1;`, `var seen: uint =
  'counter.count';`) — passes 5/5 cycles in the container.
- Tests: parser quoted-path AST test; interp E0408-without-session
  test; lexer bare-quote test. Totals: 67 tests (10 elab, 1 fixtures,
  13 interp, 15 lexer, 21 parser, 6 solve) + 4 pumpkin-gated on CI.

Verification performed: fmt/clippy/test green locally (default
features); container `verify` green with all features; container
`mixed` green using the quoted-path style.

## 2026-10-27 (8) — SN-CPU-Tutorial golden testbench: Playground attempt blocked; our baseline

The SN-CPU-Tutorial golden environment (`cpu/e/gold`, 11 files) is **pure
e**: the CPU itself is modeled in e (`cpu_dut_s` with a signal map, an
edge event `dutclk is fall(smp.clk_p$) @sys.any` and the `run_DUT()
@dutclk` TCM) — no Verilog DUT exists, so a Specman-only run suffices.
The 11 files were concatenated in dependency order (import hubs removed:
`cpu_top.e` and the `import cpu_top;` line of `cpu_tst1.e`;
`local/sn-cpu-tutorial/golden-combined.e`, 796 lines, kept out of the
repository as third-party code).

EDA Playground attempt (Specman e + SV/Verilog, Xcelium 25.03, minimal
`module design` top): **blocked before submit** — the session had expired
and the reCAPTCHA checkbox refused to tick on two coordinate clicks
(measured rect, elementFromPoint confirms the iframe is the hit target;
the widget just does not acknowledge the automated environment this
time). Per policy no CAPTCHA solving is attempted; the login fallback is
an owner login in the automation browser, after which the assembled
playground is ready to save and run. (Morning runs today logged in fine
with the same method; this rejection is a new observation.)

Our side, measured on the same assembled file
(`local/sn-cpu-tutorial/golden-combined.e`): **103 diagnostics** —
E0113×52 (unsupported members: `simple_port` fields, events, TCM
`@event` methods, `case` statements), E0105×14 / E0103×14 (port and
`define` syntax), E0101×10 (top-level `import`/`define`), plus
`as_a()` casts, bit slices (`alures[7:0]`), implication constraints
(`=>`), `in [ADD .. XORI]` ranges, `outf`/`append`, `start`, `wait [1] *
cycle`, `+=`. This is the precise delta for the M3 slice; the
comparison table (Specman log vs e1647-rs output) will be filled here
once the owner login unblocks the Playground run and the events/TCM
slice is implemented.

## 2026-10-27 (9) — language slice: `case`, ternary, `if … then`, `outf`

First slice toward running the SN-CPU-Tutorial golden testbench (and the
asic-world corpus): four construct families from the measured delta.

- `case expr { val: {..}; [a, b]: {..}; default: {..}; };` — statement
  form with grouped alternatives and a default arm; executed by
  comparing the scrutinee against each alternative.
- Ternary expressions `cond ? a : b` (loosest binding).
- `if cond then { .. }` accepted alongside `if cond { .. }`.
- `outf(fmt, args)` builtin: `%x`/`%X`/`%d`/`%s` and `%%`; missing
  arguments are a diagnostic.
- 4 new tests (3 parser + 1 execution test covering case/ternary/outf
  together). Totals: 71 local tests + 4 pumpkin-gated on CI.

Measured honestly: the assembled SN-CPU golden file still shows 103
diagnostics — its parse fails on `simple_port`/events/TCM members before
any `case`/ternary is reached, so this slice shrinks the language delta
but not yet that count. The remaining gate is unchanged: events, TCMs
(`start`/`wait`), and `simple_port` members — the next slice.

Verification: fmt/clippy/test green locally; container `verify` green
(all features); container `mixed` green (5/5 cycles, quoted-path style).

## 2026-10-27 (10) — rung 2a: events, TCM syntax, start/wait/emit, case/ternary/outf

Rung 1 (previous entry set) landed `simple_port` members with `hdl_path()`
binding and `port$` access (commit caffbf4). Rung 2a adds the events/TCM
syntax and a deterministic single-threaded execution model:

- Event members: `event clk;` and `event dutclk is <trigger>;` — the
  trigger expression (`fall(clk$) @sim`, `cycle @p_smp.clock_f`,
  `@sys.any`) is stored as written; edge scheduling is rung 2b.
- TCM methods: `name() @event is { .. }` (also `name(): type @event`);
  the event expression is stored on the method.
- Statements: `wait cycle;` / `wait [n];` / `wait [n] * cycle;` (v1
  kernel: time units and cycles advance the same counter), `emit e;`,
  `start m(...);` — `start` executes the TCM body inline with a shared
  cycle budget (`WAIT_BUDGET` 100_000 waits; runaway TCMs stop with the
  new `E0409`).
- Also landed: `case`/ternary/`if … then`/`outf` (see the sibling slice
  in this session).

Golden delta re-measured (`local/sn-cpu-tutorial/golden-combined.e`):
**103 → 83 diagnostics** — events, TCM methods, ports, `case` and
ternaries now parse; the remaining 83 are dominated by `import`/`define`
top-level statements, `when` inheritance, `%`/`!` field modifiers,
`as_a()` casts, bit slices, `in [ranges]`, `message`/`specman`/
`set_config` and `for each in` — the rung 2b/3 list.

Verification: fmt/clippy/test green locally (75 tests — 14 interp incl.
inline-TCM execution and the E0409 budget test, 24 parser, 15 lexer,
10 elab, 10 solve, 1 fixtures(18 pairs), 6 solve-translations); container
`verify` green with all features; container `mixed-port` green (5/5
cycles, signal-map style).

## 2026-10-27 (11) — rung 2b/3 slice: inline enums, defines, for-loops, in-sets, casts, slices; spmn-e-utils baseline

- Inline enumeral types: `kind : [imm, reg];` — field defaults to the
  first enumerand, bare enumerand names resolve, `gen` picks randomly.
- `define` members: `define strt_st 1;` — literal values resolve as
  global constants at run time (macro `define as`/`as computed`
  substitution remains a later milestone).
- Loops: `for each in <list> do { .. };` (binds `it`) and
  `for i from a to b { .. };` (inclusive).
- Membership: `x in [a, b]` and `x in [low .. high]` expressions; casts
  `expr.as_a(type)` (value-preserving); bit slices `base[high:low]`
  (integer extraction).
- spmn-e-utils (github.com/efratcdn/spmn-e-utils) added as the submodule
  `external/spmn-e-utils` (index repo; the utilities live in separate
  repos — 7 cloned shallowly under `local/spmn-utils/`, gitignored).
  Baseline: 17 e files probed, 1–25 diagnostics each; the four
  `e_util_*` libraries sit at 1 diagnostic (blocked on `define as` /
  `as computed` macro machinery — rung 3); the usage examples sit at
  5–25 (sequences/ports/UVC structure).

Verification: fmt/clippy/test green locally (79 tests: 14 interp incl.
case/ternary/outf execution, 24 parser incl. inline-enum + in-set +
slices, 15 lexer); container `verify` green with all features;
`mixed-port` green.

Combined SN-CPU golden delta now measured on the same file: re-run after
this slice lands the count in the entry-10 range (83) with the remaining
blockers being `when`-inheritance semantics, `import` hub resolution and
struct/`check that`-style statements — the rung 2b execution work.

## 2026-10-27 (12) — efratcdn + amiq corpora baselines; explicit `method`, event triggers, `keep bind`, `=>`, word operators

- Corpora added (cloned shallowly under gitignored `local/`, never
  committed): the remaining efratcdn utility repos (`cover_callback`,
  `cover_cb_report_hit`, `cover_cb_report_progress`, `e-utils`,
  `e2hdl_check_alignment`, `etl`, `list_mult_delete`,
  `parallel_for_each`, `reflection_examples`, `save_restart_demo`,
  `type_min_max` (empty), `hello-world`) alongside the seven from
  entry (11), and `amiq-eda/example-projects-uvm_ref_flow_1.1`
  `soc_verification_lib` (167 e files: apb/ahb/uart/spi/gpio UVCs,
  apb_subsystem, uart_ctrl). One amiq file (`uart_g5.e`) is binary
  content, not e source, and is excluded from parse probing.
- Baseline probe (`cargo build --bin e1647; e1647 parse <file>` per
  file, counting `error[` lines): 219 efratcdn+amiq files probed,
  2326 diagnostics total, down from 2609 measured before this
  entry's grammar slice. The largest per-file buckets before the
  slice were `keep bind` (82 — nearly every amiq BFM), explicit
  `method` members, `event … is @…` triggers (43), `=>` implication
  keeps (27), and `!`/`%`-prefixed fields that the member dispatcher
  never routed to the field parser.
- Grammar landed for those (parse + check + interp subset):
  explicit `method` keyword spelling and `is empty` bodies; the full
  event-trigger grammar (`@dotted.path`, `only`/`unexp`, `cycle`,
  `rise`/`fall`/`change`/`true(cond)` edge samples, `or`-composed
  items) stored as a structured `EventTrigger`; `keep bind (a, b)` /
  `keep bind x to <hdl path>` recorded as bind constraints that
  checking and `gen` skip; `=>` implication (parser + evaluation,
  short-circuiting); word-spelled `and`/`or`/`xor`/`not` operators at
  their symbol twins' precedence levels; mid-expression `soft`
  prefixes parse transparently (softness inside compound constraints
  is not modeled yet).
- Whole-library measure: concatenating each amiq UVC into one parse
  unit (approximating cross-file `import` resolution, which per-file
  probing cannot do) yields uart 253, apb 328, spi 116, ahb 183,
  gpio 100 diagnostics; the `etl` library is 22 and
  `reflection_examples` 1. Remaining construct families are ranked
  and unchanged: `on <event> { … }` actions, `cover` groups with
  `item`/`using` options, `scbd_port` members, `keep for each`,
  weighted `select` constraints, when-modifier-qualified extends and
  field types (`extend ACTIVE TX uart_agent_u`), `package` units,
  `wait until`, and `define`-macro templates.
- SN-CPU golden delta on the same combined file: 83 → 65 diagnostics
  (entry-10/11 baseline 83).

Verification: fmt/clippy clean; `cargo test` green (89 tests: 18
interp incl. implication evaluation and bind-skipping-by-gen, 29
parser incl. the five new-construct tests, 15 lexer, 10 elab, 10
solve, 6 solve-translations, 1 fixtures). Baselines above are
reproducible with `e1647 parse` per file; the concatenation measure
is `find <uvc> -name '*.e' -exec cat {} + | e1647 parse /dev/stdin`
equivalent (probe used a temporary file). The SN-CPU Playground
reference run (goal) remains parked on the manual reCAPTCHA login.

## 2026-10-29 (13) — EDA Playground reference runs vs our implementation; divergence-driven fixes

- Method: every comparison case was run on EDA Playground
  (Specman Elite + Cadence Xcelium 25.03, seed 1) through the
  Playwright automation (`local/playwright/run-corpus.mjs`,
  `finish-corpus.mjs`; logs in gitignored `local/compare/*.specman.log`),
  and the same source was run with `e1647 run --seed 1`
  (`local/compare/ours-*.log`). Divergences were fixed until the
  front-end matched or the remaining gap was a recorded milestone.
- Specman reference results captured (complete runs): `/x/Pfq` xor_try
  (20 packed operations, `check that` 0 DUT errors), `/x/D6W` SER
  ("Hello e :-) World!" + 8 driven transfers, 0 errors), `/x/5Wj` SER
  UVC (TX/RX packet collection flow), `/x/ZFqZ` UART agent (full UART
  traffic with scoreboard), `type-min-max-usage` (macros evaluate, "No
  actual running requested", 0 errors — a declarations-only case).
- The SN-CPU golden playground `/x/iCmt` reference: with the previous
  design pane, all 11 signal-map ports failed HDL binding; with a
  faithful signal shell (`top` + `cpu` submodule providing every bound
  signal) the ports bind, but Specman idles — "No @sim temporal
  expressions found". Root cause: the pin ships no Verilog DUT (the
  tutorial CPU ships with Specman installs), and the golden's events
  are all `@sys.any`-based, so nothing triggers Specman from the
  simulator. Recorded adaptation: `clkgen() @sim` in the signal map
  (one token) gives Specman its required trigger; the design shell is
  in `finish-corpus.mjs`. The re-run is parked on a fresh manual
  login.
- Divergences found and fixed (commit dbc014b, 958 insertions):
  the `as_a()` cast postfix was unreachable dead code since it landed
  (the parser rejected the keyword member name first) — now every
  primary (parens, predefined names) runs the shared postfix chain;
  predefined e names that lex as keywords (`NULL`, `it`, `index`,
  `print`, `radix`, `hex`, `dec`, `bin`, `items`, `simulation`,
  `rise`, `fall`, `change`) parse as identifiers in expression and
  statement position, and statements may assign through them
  (`it.result_from_dut = p_out$;`); `name is instance` implicit-typed
  fields; trailing `;` optional before `}` (Specman accepts both
  spellings — fixture expectation corrected); `extend global`;
  when-modifier-qualified extends and when-qualified types; `wait
  until`; grouped event triggers `(fall(a) and true(b))@clk`; `check
  that … else …`; `for i from a to b do`; field dimensions `regs[4]`;
  enumeral width suffix `] (bits: 4)`; file-level `define`/`import`;
  `+=`/`-=`. Interpreter: the root `sys` instance is addressable by
  name; data fields are implicitly generated before `run()` as Specman
  does (honoring `keep` constraints, skipping `!` and unit-typed
  fields, with `keep list.size() == N` driving list lengths);
  HDL-binding constraints (`hdl_path()`/`delayed()`) never gate gen;
  `pack(NULL, item)` packs physical fields MSB-first; `append`;
  `message(severity, …)`; `dut_error`/`error` counted and reported as
  Specman's closing DUT-error count; `stop_run()` builtin.
- Our side per case after the fixes: pfq-xor parses to zero
  diagnostics and executes gen/pack/check to the first HDL port write
  (E0408 no simulator loaded — the case needs the xezim bridge plus
  the xor_try design, which is captured in
  `local/compare/designs/`); golden-combined 83 → 65 → 13 parse
  diagnostics (remaining: `cover` groups, `expect`, `scbd_port`);
  d6w/5wj/zfqz need cross-file `import` resolution and the
  sequence/UVC construct family (their Specman references are
  captured for when those land).
- Operational notes: concurrent Playwright logins invalidate each
  other's EDA Playground session — one browser at a time; the site
  throttles rapid playground creation ("saved: /home" without a
  redirect). Remaining Playground runs (golden @sim re-run,
  cover-edges, igen-dist, save-restart-test1, etl-simple-examples) are
  parked on a fresh manual reCAPTCHA login
  (`local/playwright/login-manual.mjs`).

Verification: fmt/clippy clean; `cargo test` green (92 tests). CI
green on fdb2775 and dbc014b (check + check-solver). Specman
reference logs and design panes are gitignored under
`local/compare/`; re-runs are `node local/playwright/finish-corpus.mjs`.

## 2026-10-29 (14) — offline arc while the Playground login is parked: multi-file imports, sequence declarations, d6w runs to the reference line

- With the manual login parked, the comparison continued offline on the
  fully captured cases. The d6w playground turned out to be TWO e files
  (the imported `my_traffic.e` lives in a hidden editor tab — the pane
  capture now reads every `.CodeMirror`, not just visible ones;
  `local/compare/designs/` holds all panes of d6w/5wj/zfqz).
- Multi-file `import` resolution (commit 95de620): `import a.b;`
  resolves to `a/b.e` or `a.b.e` beside the source, depth-first; the
  well-formedness pass now runs once over the MERGED declaration set
  (`parse_unchecked` + one `check_declarations`), so cross-file
  references resolve; unresolvable imports report E0205. This closes
  the "single-file adaptation" workaround for imported corpora.
- Sequence declarations: `sequence NAME using item = T,
  created_driver = D, created_kind = K;` synthesizes the declaration
  trio (`type K: [MAIN];`, `unit D` with `item_done`/`get_next_item`,
  `struct NAME`) plus option defines that elab recovers into
  `Elaborated.sequences`; `do <item> [keeping { … }];` statements;
  `any_sequence_item` accepted as a builtin like base; `LOW`/`MEDIUM`/
  `HIGH`/`TEST_DONE` predefined names; objection raise/drop as no-op
  builtins; member-called builtins (`driver.raise_objection(...)`)
  route through the builtin table. Implicit gen and MAIN sequence
  start now happen even when no explicit `run()` exists (Specman
  test-start semantics); `do` generates the item in place — the
  get_next_item/item_done blocking handshake remains the TCM-scheduler
  milestone.
- Result: the two-file /x/D6W case parses clean across files and runs
  to the reference output line `Hello e :-) World!` (the Specman
  reference also shows 8 DUT-side `Received …` echoes, which come from
  the Verilog design via $display — comparable only through the xezim
  bridge with the captured `dut`/`top` panes).
- Remaining for a full d6w/5wj/zfqz comparison: the TCM scheduler
  (real event-driven start/wait so the driver loop and sequence
  handshake interleave), then the bridge wiring for the captured
  designs.

Verification: fmt/clippy clean; `cargo test` green; CI green on
95de620. Reproduce: `e1647 run local/corpora/d6w/testbench.e --seed 1`
(two files: `my_traffic.e` + `testbench.e` under `local/corpora/d6w/`,
gitignored; the run needs no simulator).

## 2026-10-30 (15) — remaining corpus cases green on the Playground; adaptation anatomy and our-side deltas

- After the login refresh, every remaining corpus case ran on EDA
  Playground (Specman Elite + Xcelium 25.03). Playwright discipline:
  ONE automation Chrome at a time — the ~128 accumulated
  temp-profile processes wedged every new session until they were
  killed (`local/playwright/kill-eda-chrome.ps1` matches
  `--user-data-dir=` temp paths only, leaving personal browsing
  alone).
- Reference results (`local/compare/*.specman.log`):
  `cover-edges` and `igen-dist` run CLEAN — Specman end-of-test, 0
  DUT errors, cover data written, clean `$finish` exit;
  `save-restart-test1` runs clean (0 DUT errors); `type-min-max-usage`
  runs clean (declarations-only; macros evaluate at load);
  `etl-simple-examples` fails to compile on Specman template macros
  (`No template called 'deque_iterator'` — `define as` templates, the
  same rung-3 family our parser is missing; our per-file concat also
  required stripping each component's `package etl;` line — package
  must appear exactly once);
  `sn-cpu-golden` compiles, all 11 ports bind, and Specman runs to
  the Playground runtime ceiling — the recorded no-DUT ceiling (the
  tutorial's Verilog CPU ships only with Specman installs).
- Adaptation anatomy (why the first four attempts failed, recorded
  for the next person): (1) `@sim` is only legal as
  `change/rise/fall(<hdl path>)@sim` — a bare `@sim` fails to
  compile; (2) the sampler must be an ACTIVE event whose port path
  resolves (`keep hdl_path() == "top"` + port `keep … == "tick"`, and
  the event declared in `extend sys` referencing
  `sim_smp.tick_p$`, exactly the golden's shape); (3) the added code
  needs its own `<' … '>` block — the corpus files open their blocks
  later, so unwrapped additions are prose to Specman; (4) the pure-e
  design shell needs `always` activity plus a bounded `$finish`, else
  declarations-only cases idle into the Playground runtime kill
  (exit 137).
- Our side after these runs (`local/compare/ours-*.log`, `e1647 run
  --seed 1`): save-restart 12 → 2 diagnostics (the `sequence …
  using` synthesis from 95de620; remaining: `on <event> { … }`
  blocks), igen-dist 5 (weighted `select` constraints),
  type-min-max 2 (`define as` macro templates), cover-edges 13
  (cover groups, `file`/reflection builtins), etl 22 (template
  structs/macros), golden 13 (cover/`expect`/`scbd_port`). These
  first-blocker lines are the ranked input for the next parse arcs.
- Operational: `peek-tb.mjs <url>` dumps a playground's saved panes
  (the paste-before-rerender failure mode is real — always verify
  what the site holds, never trust the intended paste).

Verification: reference logs are runs of the cited playgrounds
(Specman Elite 25.03, Xcelium 25.03, seed 1); our-side numbers are
`e1647 run` exit logs. fmt/clippy clean; `cargo test` green at the
commits of record (through 801564e); no source changes in this
entry — corpora adaptations live in gitignored `local/`.

## 2026-10-30 (16) — offline arc: clause 4.5 operator completion and clause 4.3.1.1 extend-global rejection

Scope claimed for this change (nothing broader):

- Expression precedence completes the clause 4.5 Table 12 operators the
  parser already tokenized: `is [not] a <type>` between `==`/`!=` and the
  comparisons, and the `~` / `!~` string-match operators between `===`/
  `!==` and `in`. A bare `is` not followed by `a`/`not a` is not consumed
  by the type-check level.
- Runtime semantics, honestly staged: `is [not] a` checks scalar kinds by
  value shape and named types by declared name walking `like` bases (no
  subtype-aware dispatch); string matching evaluates the case-insensitive
  full-string literal subset only — `/.../` AWK patterns and native
  meta-characters report `E0410` (the meta-character table of clause 4.11
  is an image in the local redline copy, so no meta-character semantics
  were guessed).
- User-written `extend global` is rejected with `E0116` (clause 4.3.1.1);
  the parser's file-level `define`/`import` `extend global`
  pseudo-declaration keeps parsing (fixture + unit test both ways).

No reference-simulator runs in this arc: the behavior qualified is pure
syntax/precedence, which the local tests pin, and the string-match
semantics subset is documented as unevaluated rather than claimed.
Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`, Windows):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(100 passed: 10 elab, 1 fixtures, 24 interp, 24 lexer, 35 parser, 6 solve)
```

New tests: `tests/lexer.rs::negative_string_match_is_one_token`;
`tests/parser.rs` clause 4.5 precedence placements (string match between
identity and `in`, `is a` between equality and comparisons), bare-`is`
non-consumption, `extend global` rejection, pseudo-declaration survival;
`tests/interp.rs` `is a` scalar/named/`like`/enumeral checks, literal string
match, `E0410` for AWK and meta-character patterns;
`tests/fixtures/extend_global.{e,expected}`.

## 2026-10-30 (17) — offline arc: `on` struct members (clause 11.2) and clause 21 preprocessor flags

Scope claimed for this change (nothing broader):

- `on <event-path> { actions };` struct members parse, compose through
  `like`/`extend` (a repeated declaration extends, clause 11.2's
  `is also` effect; base types first, extends in declaration order) and
  execute: `emit` now runs the event's `on` action blocks inline, in
  composition order, on the instance the event path resolves to (bare
  name → enclosing instance; dotted path → the object it reads through).
  Event match is by event name (last path segment) within the owner's
  type — clause 11.2 keeps the named event local to its struct. Each
  block runs with a fresh locals frame; `return`/`break`/`continue` end
  only that block; `emit`-inside-`on` recursion is depth-bounded
  (`E0406`). The clause's no-TCMs restriction is not enforced, and
  `@sim`-attached events never fire (unchanged).
- Preprocessor flags (clause 21.2): `#ifdef` / `#ifndef` / `#define` /
  `#undef` in statement, struct-member and action positions.
  `#define NAME <replacement>;` records the flag; the replacement text
  is consumed per the clause 21.3 single-line rule but NOT substituted
  (replacement rules are a later milestone). `#undef` takes `;` or the
  line end. Excluded branches are skipped without parsing ("ignore it").
  Conditional forms at module level report `E0117` (clause 21.2 lists
  statement/member/action positions). `#else` is not parsed — its exact
  syntax example is an image in the local redline copy and was not
  guessed.

Corpus movement (`local/`, `--seed 1`): save-restart-test1 2 parse
diagnostics → 0 (parse-clean; it was this arc's first-blocker).
The run now advances into the MAIN sequence body and stops at
`E0407` "no solution found for the constraints of 'packet'": packet's
`keep tail in [1..100]` over an unbounded `int` field. Root cause
(diagnosed, not fixed in this arc): `sample_type` samples unsized `int`
over the full 32-bit domain, so rejection sampling essentially never
hits a bounded range, and `solve.rs::translate_atom` accepts only
binary comparisons — `Expr::InSet` is `Unsupported` on every backend.
This is the next first-blocker: a clause 10 (constraints/`gen`) arc —
set-membership constraints in the solver IR and/or constraint-aware
sampling. Other corpus parse counts unchanged (etl 22, igen-dist 5,
type-min-max 2, cover-edges 1 lexical, d6w 0).

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`, Windows):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(107 passed: 11 elab, 1 fixtures, 26 interp, 24 lexer, 39 parser, 6 solve)
```

New tests: `tests/parser.rs` `on` member shape (bare and dotted paths),
hash directive include/exclude at member and action level (garbage inside
an excluded branch produces no diagnostics), module-level conditional
rejection; `tests/elab.rs` `on` composition order through extends;
`tests/interp.rs` emit runs `on` blocks in order through a dotted emit,
and emit without `on` blocks is silent;
`tests/fixtures/on_member.{e,expected}` (OK) and
`tests/fixtures/ifdef_skips_excluded_code.{e,expected}` (OK — the
byte-exact contract that excluded code is ignored, not parsed).

## 2026-10-30 (18) — offline arc: clause 10 set-membership constraints; save-restart runs to completion

Scope claimed for this change (nothing broader):

- Solver IR: `x in [lo..hi]` translates to the conjunction of the two
  comparisons (`x >= lo && x <= hi`), with both bounds through the
  existing affine translation (`2*2`, `a_max - 1` work). Item-list
  membership (`x in [a, b]`) needs a disjunction the IR does not have
  and reports `Unsupported("set membership over an item list")` — the
  caller falls back to the rejection sampler, which enforces it by
  evaluation.
- Rejection sampler: `gen_object` narrows each field's sampling domain
  to the intersection of the type's constant range/equality constraints
  (`f in [lo..hi]`, `f == c` either orientation, conjuncts combined,
  instance-independent bounds only). Without this, an unsized `int`
  samples the full 32-bit domain and a bounded range is essentially
  unhittable in 1000 attempts — the root cause of the corpus E0407.
  Contradictory domains fall back to natural sampling and fail the
  attempt honestly; unconstrained fields and non-integer scalars are
  untouched; soft keeps feed the same predicate they always did, so
  their domains narrow consistently.

Corpus movement (`local/`, `--seed 1`): **save-restart-test1 now runs to
completion, exit 0, no diagnostics** — the run covers the MAIN sequence
body (21 + 70 `do packet` generations under `keep tail in [1..100]` /
`keep del in [5..77]`), the `emit sys.env.reset_done` firing `env`'s
`on reset_done` blocks, and the objections/waits, matching the recorded
Specman reference's observable behavior (0 DUT errors, clean end, no
output; the reference's `message(LOW, …)` was verbosity-suppressed
there too). Staged limits, unchanged: only `sys` `run()` methods
execute, so the `packet_seqer` driver TCM (`bfm`, `get_next_item`,
`@clock` waits) is not scheduled yet — that remains the documented
sequence/TCM milestone.

Both backends verified: the rejection-sampler path locally on Windows
(default features), and the `solver-pumpkin` path via the repository's
own Docker route (`docker compose run --rm verify` — fmt + clippy +
`cargo test --all-targets --all-features` against the bind-mounted
working tree), since the local windows-gnu toolchain cannot compile
Pumpkin's build script. New translate tests run on both paths.

Commands and results:

```
$ cargo fmt --all -- --check                       (pass)
$ cargo clippy --all-targets -- -D warnings        (pass, no warnings)
$ cargo test --all-targets                         (114 passed, 0 failed)
$ docker compose run --rm verify                   (pass: fmt, clippy
  --all-features, tests --all-features; 118 tests, 0 failed)
```

New tests: `tests/solve.rs` range-membership IR shape (two atoms,
`Ge`/`Le`), expression bounds through the affine translation, item-list
membership reporting the explicit `Unsupported`; `tests/interp.rs` gen
under `in [1..100]` on an unsized `int` (the E0407 repro), equality
constraints in both orientations, overlapping ranges intersecting to
[5..10], and unconstrained fields sampling structurally beside a
narrowed one.

## 2026-10-30 (19) — offline arc: clause 15 macro definitions recorded; type-min-max-usage parse-clean

Scope claimed for this change (nothing broader):

- Clause 15.2/15.3 macro definitions parse and are recorded
  uninterpreted: `define <name['category]> "match expression" as
  [computed] { body };` — the header (name, optional non-terminal
  category after `'`), the match-expression string (escapes preserved)
  and the braced body as raw tokens (nested braces included). `computed`
  is accepted as a word (it is not in the lexer's keyword table). The
  bodies are deliberately not parsed — a simple macro's template may
  contain constructs the parser does not know, since the macro is what
  teaches the parser about them; the match-expression grammar
  (clause 15.4–15.5) and the expansion engine are later milestones.
  Missing pattern string reports `E0118`.
- Clause 5 scalar type declarations parse and are recorded:
  `type name: <type>;` (e.g. `type addr_t: int (bits : 8);`) — the
  underlying representation is carried through the AST, the well-
  formedness check (unknown names in it are `E0203`) and elaboration
  (`ElaboratedType.underlying`). Distinct-type semantics over the
  representation are a later milestone.
- File-level `define`/`import` handling was split into two arms while
  adding the macro shape; the `extend global` pseudo-declaration
  behavior is pinned by the existing test and still holds.

Corpus movement (`local/`, absolute paths, `--seed 1`):
type-min-max-usage 2 parse diagnostics → **0** (parse-clean; the
corpus defines six macros across the `as`/`as computed` shapes and a
scalar type declaration, and invokes none of them — declarations-only,
matching the recorded Specman reference's clean load). The run exits 0
with no output, as before. Unchanged: etl-simple-examples 22 (its
blockers are clause 8 `template struct ... of <type>` declarations —
`template` is not even a lexer keyword yet — plus template
instantiations; that is the next large family), save-restart-test1 0,
igen-dist 5, cover-edges 1 (lexical).

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`, Windows;
no solver-touching changes, so the Docker `--all-features` pass is left
to CI):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(116 passed: 11 elab, 1 fixtures, 30 interp, 24 lexer, 41 parser, 9 solve)
```

New tests: `tests/fixtures/macro_definition.{e,expected}` (OK — simple,
computed and category-less shapes plus a scalar type declaration);
`tests/parser.rs` macro-definition recording (name/category/pattern/
computed flag, raw-token bodies surviving nested braces, placement in
the global pseudo-declaration) and scalar type declarations (underlying
recorded; unknown names in it rejected).

## 2026-10-31 (20) — offline arc: clause 8 template declarations (stage 1) and the etl body surface

Scope claimed for this change (nothing broader):

- Clause 8.1 template declarations parse: `template [package]
  struct|unit name of <params> [like base] { .. };` with `<[tag']category>`
  parameters (defaults recorded for trailing parameters), `package`
  prefixes recorded, and parameterized bases (`like container of
  <type>`) stored as the base name plus the full type expression.
  `template extend` (clause 8.2) and instantiation (clause 8.3) are not
  implemented; the lexer gains the 2016 keyword `template` (a documented
  deviation — the pinned reference grammar predates it).
- Type positions record template formals (`<type>` — the raw text
  between the brackets; formal names may be keyword spellings such as
  `type`), template references (`deque of int`, parenthesized
  multi-argument forms) and keyed lists (`list (key:it) of t`, key
  expression recorded). Unknown template names are `E0203`, like unknown
  types.
- The etl corpus's body surface, surfaced once headers parsed:
  `var x := e` type-inferring declarations (`:=` operator); `new
  [<type>] [with { assignments }]` parsed, evaluating to an explicit
  `E0402` not-supported diagnostic; `compute <call>;` statements
  (executed inline like `start` — clause 13's run-to-completion
  semantics needs the TCM scheduler); `for i from a down to b`;
  `else if` chains; `for each (v) using index (idx) in reverse list`
  (index binding = element position; the `using` name recorded);
  `is undefined` body-less methods; untyped parameters (`add(packet)`);
  `is a <type> (binding)` bindings (a true match binds the value for
  the branch body); last-statements-of-a-block may omit `;`; method
  and field names may be Table 8 keyword spellings (`set(...)`,
  `item: <type>`, `prev: ...`); `package`/`private` member prefixes
  recorded on fields (clause 22, not enforced).

Corpus movement (`local/`, absolute paths, `--seed 1`):
etl-simple-examples 22 parse diagnostics → **4**, all four the same
honest `E0203` — `base_struct` is declared in the etl library's other
file, absent from this single-file corpus extract, exactly like the
reference's own missing-dependency class of failure (the recorded
Specman run of this file also failed to compile, on template macros).
Everything else unchanged: type-min-max 0, save-restart 0 (run still
clean), igen-dist 5, cover-edges 1, d6w 0.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`, Windows;
no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(123 passed: 11 elab, 2 fixtures, 35 interp, 24 lexer, 45 parser, 6 solve)
```

New tests: `tests/fixtures/template_declaration.{e,expected}` (OK —
nested template references, package/private prefixes, defaults,
`new ... with`, `var :=`, keyed lists, `is undefined`, scalar type
declarations); `tests/parser.rs` template parameter/base recording,
formal-and-keyed-list type shapes, is-a bindings;
`tests/interp.rs` `down to` counting, reverse for-each with index
binding, is-a binding visibility, untyped parameters with `compute`
inline calls, and the explicit `E0402` for `new`.

## 2026-10-31 (21) — offline arc: clause 14 coverage groups recorded; cover-edges and golden parse surfaces

Scope claimed for this change (nothing broader):

- Clause 14.1 coverage groups parse and are recorded, **not collected**:
  `cover <event> is [also] { ... };` and `cover <event> is empty;`.
  Items parse structurally — `item name [: type] [= expr] [using ...];`
  with the optional declared type and sampled expression — plus
  `cross a, b;` and `transition f;` entries; every `using` option
  clause (brace-aware: inner `range(...);` semicolons stay inside the
  capture) and every group-level option entry (`per_instance = TRUE;`)
  is recorded verbatim. The `is also` extension form (clause 14.5) is
  recorded. Nothing is collected at run time: `emit` of a sampling
  event behaves exactly as before; the coverage data model (options,
  buckets, grades, reports) is a later milestone.
- Predefined non-scalar types `file` and `event` are accepted in type
  positions (`var f: file`, `ev: event;`) via a `BUILTIN_TYPES` list in
  `builtins.rs`, checked like declared names; `for each <type> (v) in
  list` records the element type filter (not enforced); stray empty
  statements (`;;`) are tolerated in statement lists.

Corpus adaptations (gitignored `local/` only, recorded per the ledger
discipline): cover-edges.e line 136 `'>File name : ...` split into the
end-code marker plus a prose line — clause 4.1.2 requires **both**
markers to have no other text on the line, so the lexer was right and
the file was non-conforming; golden-combined{,-imports}.e's
`extend global { setup_test() ... }` — the Specman runtime's own system
hook concatenated into the tutorial extract — re-pointed at `sys` with
a comment (clause 4.3.1.1 reserves `extend global` for the runtime; our
interp never calls the hook).

Corpus movement (absolute paths, `--seed 1`): cover-edges 1 lexical +
13 semantic parse diagnostics → **4**, all `E0203` for the Specman
reflection library's own types (`rf_struct`, `rf_type`, `rf_field`) —
a missing external dependency, same class as etl's `base_struct`;
golden-combined ~20 → **2** (the one `expect` temporal member, clause
11.4, which needs the clause 12 grammar — the next arc);
golden-combined-imports → 3 missing tutorial-module `E0205`s plus the
same `expect`. Unchanged elsewhere: etl 4, type-min-max 0,
save-restart 0 (run clean), igen-dist 5, d6w 0.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`,
Windows; no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(125 passed: 11 elab, 1 fixtures, 34 interp, 24 lexer, 46 parser, 9 solve)
```

New tests: `tests/parser.rs` cover-group recording (item kinds
including cross/transition, typed and sampled items, brace-aware
verbatim option captures, group option entries, `is also`, stray
`;;`), typed for-each filters and `file`/`event` types; the stale
`unsupported_member` fixture (whose example was `cover` itself) now
pins `assert` members (clause 16), the remaining unsupported-member
family.

## 2026-10-31 (22) — offline arc: clause 11.4 expect/assume members recorded; golden-combined parse-clean

Scope claimed for this change (nothing broader):

- `expect`/`assume` struct members (clause 11.4) parse and are recorded
  **uninterpreted**: the temporal expression is captured as raw tokens
  up to the `else` keyword or a top-level `;` at brace depth zero
  (TE-internal braces and their semicolons stay inside the capture);
  the `else` failure body parses as ordinary actions in both the
  one-statement form (which consumes its own terminator) and the braced
  form. An empty temporal expression reports `E0119`. The clause 12
  temporal-expression grammar, ports, and the evaluation engine are
  later milestones — nothing is evaluated at run time, and the members
  are not collected during elaboration.
- `E0119` added for the missing temporal expression.

Corpus movement (absolute paths, `--seed 1`): **golden-combined 2 → 0
parse diagnostics — every corpus file in `local/` now parses clean or
carries only external-dependency diagnostics** (etl 4 `base_struct`,
cover-edges 4 rf-library types, golden-imports 3 missing tutorial
modules, igen-dist 5 weighted-select constraints, d6w/type-min-max/
save-restart 0). The recorded Specman run of the golden needed the full
Specman install; our parse of the standalone extract is complete.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`,
Windows; no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(127 passed: 11 elab, 1 fixtures, 34 interp, 24 lexer, 48 parser, 9 solve)
```

New tests: `tests/fixtures/expect_member.{e,expected}` (OK — the
golden's expect shape plus an assume with a braced else);
`tests/parser.rs` expect/assume recording (kind, raw-TE content with
the `else` keyword excluded, one-statement and braced else bodies) and
the no-else form plus the `E0119` empty-expression diagnostic.

## 2026-10-31 (23) — offline arc: clause 10 select distributions recorded; igen-dist parse-clean and running

Scope claimed for this change (nothing broader):

- `select { <weight> : <choice>; .. }` distribution expressions
  (clause 10) parse: `Expr::Select` with per-arm weight expressions
  (structural) and choice tokens recorded verbatim (paren-aware, so
  `normal(MAX_UINT/2, MAX_UINT/10)` keeps its arguments; `min`, `max`,
  `others`, `edges`, `normal(..)` and value/range expressions all
  land). Stray empty statements inside the block are tolerated.
- Runtime: select constraints are no-ops for the rejection sampler —
  the pre-existing `expr_calls_any(.. "select")` skip now covers the
  structured `Expr::Select` variant — so `gen` produces uniform values
  where the reference biases; a select expression evaluated directly
  (outside a skipped constraint) reports the honest `E0402`
  "select distribution expressions are not evaluated yet". The
  distribution arc (weights, `normal` sampling, edges) is a later
  milestone; `solve.rs` translation reports such constraints as
  unsupported and falls back, as before.

Corpus movement (absolute paths, `--seed 1`): **igen-dist 5 → 0 parse
diagnostics** and the run completes, exit 0 — with the sampler
treating the distributions as no-ops, honestly recorded. Every corpus
file in `local/` now parses clean or carries only
external-dependency diagnostics (etl 4 `base_struct`, cover-edges 4
rf-library types, golden-imports 3 missing tutorial modules;
golden-combined 0; type-min-max/save-restart/d6w 0).

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`,
Windows; no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(130 passed: 11 elab, 1 fixtures, 36 interp, 24 lexer, 49 parser, 9 solve)
```

New tests: `tests/fixtures/select_constraint.{e,expected}` (OK — the
igen-dist shapes plus a value-list choice);
`tests/parser.rs` select recording (arm weights, verbatim
paren-aware choices, the equality against `select`);
`tests/interp.rs` select-as-sampler-no-op (gen still produces
in-range values) and the honest direct-eval diagnostic.

## 2026-10-31 (24) — offline arc: clause 12 temporal-expression grammar structured behind expect/assume

Scope claimed for this change (nothing broader):

- `expect`/`assume` members now parse their temporal expressions into
  the clause 12 grammar (`TemporalExpr`) instead of recording raw
  tokens: `@event` atoms with dotted, `$`-suffixed paths (12.2.4),
  `cycle` (12.2.2), `true(exp)` propositions (12.2.3),
  `change/rise/fall(exp)` edge samples (12.2.17), `{a; b}` sequences
  (12.2.8), `=>` yield (12.2.13), `and`/`or` (12.2.6–12.2.7),
  `not` (12.2.16), `fail` (12.2.12), `eventually` (12.2.14), the
  `[n]*te` / `~[l..h]*te` / `[l..h]*te` repeats (12.2.9–12.2.11), the
  binary sampling operator `te @event` whose operand is a bare dotted
  event path (12.2.5), and `( te )` grouping. `detach`, `delay` and
  `exec` TEs are not supported and report `E0120` (which also covers a
  missing temporal expression; the previous arc's `E0119` is reserved).
- Precedence, loosest to tightest: `=>`, `or`, `and`, the sampling
  suffix, repeats/prefix operators, atoms — Table 37 is an image in
  the local redline copy, so this order follows the prose conventions
  and is documented in the parser. The golden's
  `@enter_exec_st => {@fetch1_assert} @sys.cpuclk` structures as
  Yield(@enter_exec_st, Sampled(Sequence{@fetch1_assert}, sys.cpuclk)):
  the sampling attaches lexically to the yield's rhs, and the
  semantic propagation to both sides (12.1.4 sampled normal form,
  12.2.13) belongs to the temporal engine.
- Nothing is evaluated: the members are still not collected during
  elaboration, and the temporal engine (sampling propagation 12.1.4,
  success/failure over paths 12.3, `wait`-integration) is a later
  milestone.

Corpus state (absolute paths — the relative-path corpus check bit the
stale-cwd trap again this arc): golden-combined 0, igen-dist 0,
type-min-max/save-restart/d6w 0, etl 4, cover-edges 4, golden-imports 3
— all four-family counts are the external-dependency diagnostics, no
regressions.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`,
Windows; no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(131 passed: 11 elab, 1 fixtures, 36 interp, 24 lexer, 50 parser, 9 solve)
```

New tests: the expect/assume shape test rewritten for the structured
tree (yield under sampling, event paths, sequence items); a new
clause-12-operators test covering sequences with first-match repeats,
true-match repeats over propositions, edge-sample or-groups with
`not`, `fail` + `eventually`, and `[2]*cycle` under sampling; the
missing-TE diagnostic updated to `E0120`.

## 2026-10-31 (25) — offline arc: clause 12 temporal-engine core (sampling propagation 12.1.4, path evaluation 12.3)

Branch `agent/temporal-engine` (off merged main; the nine stacked PRs
#6–#14 landed in this session's review-and-merge pass). The temporal
engine stage: a pure evaluation core for the clause 12 `TemporalExpr`
trees the grammar arc introduced.

- New `src/temporal.rs`: `sampled_normal_form(te, q)` propagates the
  sampling event to the sampled normal form `S(t, q)` (12.1.4) —
  matching the clause's worked example `{@a; ~[..]*cycle; @b} @q` →
  `{@a @q; ~[..]*cycle @q; @b @q} @q`: each direct sub-expression
  carries the event attached as a unit, sequences keep the outer
  sampling on the braces, repeats are sampled as units with their
  interior untouched, and a sub-expression with its own sampling keeps
  it (12.2.5's override). `holds_tightly`/`succeeds` evaluate tight
  satisfaction (12.1.1.2) and last-point success (12.1.1.12) over
  explicit finite paths of `State`s (each = the events that occurred in
  one sampling period), with propositions and numeric repeat bounds
  evaluated through caller-supplied callbacks
  (`EvalCallbacks`; no interpreter wiring yet).
- Operator semantics per subclause, documented inline: event atoms and
  `cycle` tight on a single state (12.2.4, 12.2.2 — the atom `cycle`
  holds at any state, 12.1.2), `true(exp)` via callbacks (12.2.3),
  sequences over strictly consecutive segments (12.2.8), `and`/`or`/
  `not` (12.2.6/7/16), `fail` = no success possibility on the slice
  (12.2.12), `eventually` (12.2.14), yield as a universal obligation
  over the lhs's successful prefixes with vacuous hold when the lhs
  fails and failure when the rhs has no room after a last-point lhs
  success (12.2.13), fixed `[n]*` unrolling with `[0]*` succeeding
  immediately (12.2.9), true-match `~[l..h]*` as a k-range of
  consecutive-success unrollings with `~[..]*` starting at zero
  (12.2.10), first-match repeats as sequence items expanding to k
  copies + rest with an omitted lower bound at zero (12.2.11's
  `{@ev_a} or {[1]*TE1; @ev_a} ...` equivalence).
- Re-classified the repeat spellings per the extracted 12.2.9–12.2.11
  prose (the grammar arc had labeled every un-tilde bracket repeat
  first-match): `[n]*te` with a single bound is the **fixed** repeat
  (12.2.9's own example is `wait [2]*cycle`), `~[n]*te` the
  exact-count true-match form (`n..n`), and only a `..` range without
  `~` is the first-match repeat. The parser suite's sequence member now
  uses 12.2.11's `{[2..4]*@ev; @clk}` shape and pins `[2]*cycle` as
  Fixed.
- `parse_temporal_expression` is a public entry point that parses one
  standalone `TemporalExpr` (used by the engine tests; expect/assume
  members parse through the same grammar).
- Honest gaps: `change/rise/fall(exp)` edge samples report
  `EvalError::EdgeSample` (they need a sampled value history across
  states — the pure core has none), unresolvable repeat bounds report
  `EvalError::Unsupported`, and nothing touches simulation time —
  driving the engine from `wait`/`emit`/expect monitoring over the run
  loop is the next temporal milestone.

Corpus state (absolute paths; baseline unchanged): golden-combined 0,
igen-dist 0, type-min-max/save-restart/d6w 0, etl 4 (E0203
base_struct), cover-edges 4 (rf-library types), golden-imports 3
(missing tutorial modules) — no regressions.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`,
Windows; no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(139 passed: 11 elab, 1 fixtures, 36 interp, 24 lexer, 50 parser,
 9 solve, 8 temporal)
```

New tests (`tests/temporal.rs`, 8): the 12.1.4 worked example's normal
form shape (items sampled as units, outer sampling retained on the
sequence, repeat interior bare); sub-sampling override (12.2.5); atom/
sequence/cycle holds including the glued-state rejection and the
12.1.1.2-vs-12.1.1.12 distinction on `cycle @q` (tight on the
single-state path only, yet succeeds at the last point of a two-cycle
path via the final cycle — the exact illustration 12.2's preamble
draws); yield/not/fail distinction with the golden's
`@enter_exec_st => {@fetch1_assert} @clk` shape over expect-style
tight satisfaction; fixed/true-match repeats over propositions and
`eventually` + first-match `{[..]*@a; @b}`; edge-sample error;
unresolvable-bound error.

## 2026-10-31 (26) — offline arc: clause 12 temporal engine wired to the run loop (wait, emit log, expect monitoring)

Branch `agent/temporal-runloop` (off main after #15 merged). The
temporal-engine core now has a driver: the interpreter keeps a run
log — one `State` per virtual cycle recording which events occurred —
and `emit`, temporal `wait`s and `expect`/`assume` rules read and
write it.

- `emit` (11.2) additionally records the occurrence in the current
  cycle's state. `emit e; wait @e;` succeeds in the same cycle
  (12.3.1's sub-path over the recorded state).
- Temporal `wait` statements parse (13.1.3): `wait @event`,
  `wait {..} @clk`, `wait true(exp) @clk`, `wait [n]*@event`,
  `wait until <te>` — the standard's NOTE makes `until` readability
  only, so it is the same temporal wait; `cycle`/`[n]`/`[n]*cycle`
  stay the numeric wait, and a plain-expression `until` keeps the v1
  advance-one behavior. Execution advances one cycle at a time and
  re-checks `succeeds` (12.3.1) over the log from the wait's start;
  a wait unsatisfied after 1,000 cycles (or the shared budget)
  reports `E0409`; unevaluable TEs (`change/rise/fall`, unresolvable
  bounds) report `E0402` honestly.
- `expect`/`assume` members (11.4) elaborate per type
  (`ElaboratedType.expects`, base-first then extends) and are
  monitored at quit: `rule_failed` (12.3.2 — the rule fails where
  `fail te` succeeds) reads per shape — yields fail when an lhs
  success left no rhs success after it (12.2.13's "in sequence"),
  `eventually` and other success-required shapes fail when they never
  succeeded anywhere (12.3.1 at any point via `succeeded_somewhere`),
  `not`/`fail` shapes are invariants. A failed rule runs its `else`
  body on the sys instance (whose `dut_error` reports as usual); with
  no else body the failure itself prints and counts as the check
  error.
- Honest approximations, documented in the code and ARCHITECTURE: one
  global timeline (all events share per-cycle states; the sampling
  event's occurrences are every cycle boundary, so 12.3.3's legal
  starts are any index), propositions and repeat bounds evaluate once
  per check with current field values (no per-cycle value history —
  the log records event occurrences only), nothing executes
  concurrently during a wait (TCM suspension is the next temporal
  milestone), and a temporal wait can therefore only observe
  emissions recorded before it started or the padded later cycles.

Corpus state (absolute paths): golden-combined 0, igen-dist 0,
type-min-max/save-restart/d6w 0, etl 4 (E0203 base_struct),
cover-edges 4 (rf-library types), golden-imports 3 (missing tutorial
modules) — unchanged. `e1647 run` of golden-combined still exits 0
with only the pre-existing implicit-gen E0407; its `@sys.any`-based
expect rules stay vacuous (sys.any never fires in this model) so
monitoring is silent there.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`,
Windows; no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(149 passed: 11 elab, 1 fixtures, 45 interp, 24 lexer, 51 parser,
 9 solve, 8 temporal)
```

New tests: 9 interpreter tests (emit feeds the log and a same-cycle
wait sees it; an `on` block's numeric wait lands the emission in the
next cycle's state and `wait until @ev` sees it; the unsatisfied
temporal wait hits the budget guard without completing; a failed
`@start => eventually @done` expect runs its `dut_error` else body
and counts one DUT error; satisfied and vacuous rules stay silent; a
failed rule with no else body prints the failure as a check error;
`expect eventually @done` fails at quit when done never occurred;
`wait {@a; cycle}` spins into the padded next cycle and succeeds;
`wait true(x == 3)` evaluates the proposition with current field
values); a parser test splitting the nine wait spellings into
numeric vs temporal shapes; and a clean-parse fixture
(`temporal_wait.e`) locking the expect/assume + temporal-wait syntax.

## 2026-10-31 (27) — offline arc: clause 13.1.1 TCM scheduler (concurrent TCMs, real suspension)

Branch `agent/tcm-scheduler` (off main after #16 merged). `start` now
schedules instead of running inline: the clause 13.1.1 semantics —
interleaved TCMs suspending at synchronization actions — execute over
the run loop.

- Scheduler: the `run()` bodies are the main coroutine; every
  `start`ed TCM joins the queue with the implicit "sync cycle" before
  it commences (13.1.1's bullet). Round-robin turns run atomically
  from one synchronization action to the next; when no coroutine is
  eligible, one cycle passes (log padding included). The shared
  runaway budget dropped from 100,000 wait statements to 5,000
  wait/cycle draws — every suspension cycle draws from it, and 5,000
  still exceeds any corpus/test run by orders of magnitude (`E0409`
  unchanged).
- Suspension/resumption: a synchronization action in a coroutine turn
  captures the locals frame and builds a skip-cursor over enclosing
  `while` chains. Resumption re-enters the mid-iteration loop WITHOUT
  re-checking its condition (13.1.1: the iteration was in flight),
  skips the already-executed statement prefix (no replay — side
  effects do not repeat), and re-executes the wait statement against
  the grown log (13.1.1(b): evaluate the TE first — succeed resumes
  immediately, undecided stays suspended). Temporal suspensions stay
  idle until the next sampling occurrence (13.1.1(b)3); the per-wait
  runaway guard (1,000 cycles) applies to suspended waits.
- The marquee behavior now holds: a TCM suspended at `wait @ev`
  observes an `emit` made by another TCM's turn and resumes. Tested:
  waiter/emitter interleave ordering; run()-waits suspend while
  started TCMs progress; TCM locals persist across suspensions in a
  `while` loop (the captured frame); the implicit sync cycle (the
  starter continues before the TCM's first statement); `compute`
  stays run-to-completion inline (13.1.1's "called TCM returns →
  callee resumes immediately").
- `sync` (13.1.2) parses in bare (`sync;`/`sync cycle;`) and TE
  forms; the bare form suspends one cycle, the TE form behaves like
  the temporal wait.
- Honest v1 limitations (documented in ARCHITECTURE): suspension
  re-entry models `while` chains only — waits inside
  `if`/`case`/`for` bodies, inline method calls, `compute` and `on`
  blocks keep the previous inline spin (they do not interleave); one
  global timeline (no per-instance sampling); propositions evaluate
  with current field values; `@`-triggered TCM invocation and `@sim`
  sampling are not wired.
- Debugging note for the record: the first scheduler version lost the
  `while` context marker on resume — a re-suspension inside the
  loop's continuation built a cursor without the loop index, and the
  next resume skipped past the whole loop (a locals test caught it:
  the count came out 2 instead of 3). The marker now persists across
  the loop's continuation.

Corpus state (absolute paths): etl 4, cover-edges 4, golden-imports 3,
golden-combined/igen-dist/type-min-max/save-restart/d6w 0 — unchanged.
Run parity checked against a main worktree: save-restart and
igen-dist produce identical (empty) output with exit 0; golden-combined
still exits 0 with only the pre-existing implicit-gen E0407.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`,
Windows; no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(155 passed: 11 elab, 1 fixtures, 50 interp, 24 lexer, 52 parser,
 9 solve, 8 temporal)
```

New tests: 6 interpreter tests (start schedules while the starter
continues; the waiter/emitter marquee with ordering; run-waits suspend
while started TCMs progress; locals persist across suspensions in a
while loop; bare sync suspends one cycle and the post-sync wait sees a
later emission; compute stays inline), the inline-start test rewritten
onto `compute` (the old test encoded the pre-scheduler approximation),
a parser test for the four `sync` spellings, and the temporal-wait
fixture extended with `sync` forms and a suspending `while` TCM.

## 2026-10-31 (28) — offline arc: clause 14 coverage data model (buckets, hits, grades)

Branch `agent/cover-data` (off main after #17 merged). Cover groups
now collect instead of only recording.

- New `src/coverage.rs`: the data model built from the recorded
  groups at run start. `is also`/`empty` extensions merge into their
  base group by name (14.5, either declaration order). Items' `using
  ranges = { range([lo..hi], "label"); ... }` clauses — recorded as
  raw tokens — parse into explicit buckets; the corpus shape
  `range([0x21..0xffdf], "", 4000, 10)` parses with the numeric
  parameters beyond bounds/label recorded but not applied (their
  semantics live in Table 41, an image in the local redline copy).
  Items without a ranges clause take the 14.2 prose defaults: 16
  buckets for integers ("buckets are created by default for only the
  first 16 possible values (0 through 15)") and the two values for
  booleans; anything else is ungradeable with an honest note.
- Collection: the interpreter samples a group when its sampling event
  fires (14.1) — the emitting instance's type owns the groups, each
  item's recorded expression evaluates in the emitting frame (or the
  named field is read from the owner), and the value lands in the
  containing bucket. Values outside every bucket are ungradeable
  (14.2: "those values are given goals of 0 and do not affect the
  grade"). Grades are covered-buckets-per-item, averaged over
  gradeable items; a group with no gradeable items reports no grade.
- Reporting: `e1647 run --coverage` prints per group the sample count
  and grade, per item its buckets with hits against goals, and the
  ungradeable count with the item's note. Collection happens on every
  run; the flag only controls printing (default output unchanged —
  the recorded golden/igen parity runs stay byte-identical).
- Honest gaps: cross/transition items, per-instance coverage (14.2.1),
  the option tables (at_least/weight/etc. beyond the range goal) and
  the 28.9 coverage configuration are recorded, not applied; enums,
  strings, reals and lists are counted but not graded.
- Debugging notes for the record: the first ranges parser passed a
  subslice but indexed it with the absolute cursor (every range entry
  failed to parse — caught by the explicit-buckets test), and the
  group's sample counter incremented per item instead of per sampling
  event (caught by the defaults test's sample count).

Corpus state (absolute paths): etl 4, cover-edges 4, golden-imports 3,
golden-combined/igen-dist/type-min-max/save-restart/d6w 0 — unchanged.
`run --coverage` on golden-combined prints its groups' default buckets
and honest notes; the golden's run still exits 0 with only the
pre-existing implicit-gen E0407.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`,
Windows; no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(159 passed: 11 elab, 1 fixtures, 54 interp, 24 lexer, 52 parser,
 9 solve, 8 temporal)
```

New tests: 4 interpreter tests — explicit ranges buckets sampled on
emit with an out-of-range ungradeable value and a full grade; the
14.2 defaults (16 integer buckets, two boolean buckets, the plain
average group grade); `is also`/`empty` merging into one group; an
untyped unranged item recording two samples with no grade and its
honest note.

## 2026-11-01 (29) — offline arc: clause 10 select distributions bias the sampler

Branch `agent/select-biasing` (off main after #18 merged). `select`
distributions now bias the rejection sampler instead of being no-ops.

- Semantics per the extracted 10.2.12.2 prose: a select specifies the
  relative probability that a value or set of values is chosen from
  the current range of legal values ("the range of values as reduced
  by hard constraints and by soft constraints that have already been
  applied"); an arm is chosen with probability weight/(sum of all
  weights), weights being integers. The choice keywords' example
  figures are images in the local copy, so their readings follow the
  igen-dist corpus usage: `min`/`max` take the current range's
  extremes, `edges` picks one of them, `others` draws uniformly
  strictly inside, and `normal(center, stddev)` draws a Box–Muller
  normal (seeded, deterministic) clamped into the range.
- Mechanism: `keep [soft] <field> == select { .. }` (either operand
  order) narrows the field's domain to the drawn value — the same
  equality narrowing a constant comparison uses — while the rejection
  predicate keeps skipping select-shaped constraints, so the bias is
  their whole effect. The current range is the domain the other
  constraints already narrowed, or the field type's natural range
  when none did.
- Choice arguments evaluate as constant integer expressions over the
  recorded tokens: literals (decimal/based), the predefined
  `MAX_UINT`/`MAX_INT` constants, `+`/`-`/`*`/`/` with precedence and
  parentheses — the igen-dist shape `normal(MAX_UINT/2, MAX_UINT/10)`
  now biases. An unevaluable arm (unknown name, bad shape) leaves the
  field unwidened: the sampler draws naturally and no error is
  reported. In action position (`var x = select ...`) the honest
  not-evaluated diagnostic is unchanged.
- Honest gaps: the full choice-expression grammar (list/set choices,
  methods), soft-order interactions between multiple selects on one
  field, and per-instance weight expressions are not modeled; the
  drawn value clamps into the range rather than redistributing
  probability mass.

Corpus state (absolute paths): etl 4, cover-edges 4, golden-imports 3,
golden-combined/igen-dist/type-min-max/save-restart/d6w 0 — unchanged.
igen-dist still runs to completion with exit 0 (its select policies
live under `when` subtypes, which are carried, not resolved, so their
constraints are not yet active — the biasing engages once `when`
elaboration lands).

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`,
Windows; no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(166 passed: 11 elab, 1 fixtures, 61 interp, 24 lexer, 52 parser,
 9 solve, 8 temporal)
```

New tests: 7 interpreter tests — dominant weights driving the draw to
min then max; `others` strictly inside the range; `edges` at one of
the extremes; `normal` around its center; the corpus shape
`normal(MAX_UINT/2, MAX_UINT/10)` over an unsized uint; the select on
either operand side; and an unevaluable choice leaving the field free.

## 2026-11-01 (30) — offline arc: clause 6.6 when-subtype resolution (synthesized views)

Branch `agent/when-elab` (off main after #19 merged). `when` subtypes
resolve: the parsed blocks become first-class types the interpreter
instantiates.

- Parser: a when-qualified field type (`f: NO_CONSTRAINT env` — the
  subtype modifier naming the variant of the base type) previously
  DROPPED the qualifier (`ty: Named{env}`); it now records
  `TypeExpr::WhenQualified { value, base }`. The optional `is` before
  a `when` body (6.6.1) parses too.
- Elaboration: each single-modifier `when` block synthesizes a subtype
  view named `VALUE base` (the field-type spelling) — the base's
  composed members with the when block's fields, constraints,
  methods, `on` blocks, cover groups and expects appended, `base`
  pointing at the base type, and a `when_of` marker recording the
  discriminator (field id, value). Both modifier spellings carry the
  discriminator: `when BUSY'kind` (value'field) and the corpus's
  space form `when NO_CONSTRAINT env` (two bare modifiers — value
  first, field id second, per the igen-dist usage).
- Interpreter: a when-qualified field instantiates the synthesized
  view (`default_for`, `sample_type`), so the view's members apply
  for that instance — when fields exist on it, when constraints gate
  its `gen` (select distributions included: a `LO pkt` field biased
  to min and an `HI pkt` to max), when methods resolve through the
  instance's view, and `is a` checks walk the view's base chain.
- Honest v1 limits (documented): chained modifiers and nested
  when-in-when blocks do not compose; base-typed instances do not
  dynamically match subtypes (the subtype rides the when-qualified
  field's static type); a gen of a base-typed field does not consult
  an enclosing instance's when constraints (the CFS scoping of
  10.2.7).
- Forward effect recorded in NOTES: igen-dist's select policies sit
  under `when ... env` blocks — with views synthesized, activating
  them for runs is now a generation-scoping step, not a
  representation step.

Corpus state (absolute paths): etl 4, cover-edges 4, golden-imports
3, golden-combined/igen-dist/type-min-max/save-restart/d6w 0 —
unchanged; igen-dist and golden-combined runs keep exit 0 with the
same observable output.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`,
Windows; no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(169 passed: 12 elab, 1 fixtures, 63 interp, 24 lexer, 52 parser,
 9 solve, 8 temporal)
```

New tests: an elaboration test (the synthesized view's name,
`when_of`, base, and appended constraint — the base stays unmarked);
and two interpreter tests — when-qualified fields generating under
their when-block select constraints (min view vs max view), and a
when view carrying its method, its extra field and the `is a` walk.

## 2026-11-01 (31) — offline arc: clause 8.3 template monomorphization

Branch `agent/template-mono` (off main after #20 merged). Template
instances become structs: the recorded template declarations stop
being inert.

- Canonical naming (8.3's "any two instances of a template, if their
  actual parameters refer to exactly the same types, are considered
  to be the same instance"): `TypeExpr::render` gives every type a
  canonical textual form, and an instance's name is
  `name of (arg, arg)` over the rendered actuals.
- Monomorphization worklist in the elaborator, seeded from every
  type's member types (fields, method signatures) and fed by
  substitution: each `(template, actuals)` materializes an
  `ElaboratedType` under the canonical name — the template's composed
  members (like chains and extends included) with the actuals
  substituted for the formals in field types and method signatures,
  trailing parameters filled from their defaults, defaults that
  reference earlier formals substituting through (`<value'type> =
  <key'type>` under `map of (int)` binds both to the actual), and the
  parameterized base substituted to the view's base name (another
  instance's canonical name when the base is itself a template
  reference — the worklist materializes it first).
- Interpreter: template-typed fields (`TypeExpr::TemplateRef`)
  instantiate and generate the canonical view (`default_for`,
  `sample_type`), methods resolve through the instance's type name,
  and `is a` accepts both the canonical instance name and the base
  chain beneath it.
- Honest v1 limits (documented): method bodies keep their written
  form — a `new deque_iterator of <type>` inside a template body
  substitutes in a later milestone (the instance degrades to an
  empty-typed object, no crash); template extensions (8.2) apply
  wholesale via name composition without the 8.3 category-compliance
  checks; non-scalar/category validation of actuals is not performed.

Corpus state (absolute paths): etl 4, cover-edges 4, golden-imports
3, golden-combined/igen-dist/type-min-max/save-restart/d6w 0 —
unchanged; golden-combined still runs exit 0 with the pre-existing
implicit-gen E0407.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`,
Windows; no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(172 passed: 14 elab, 1 fixtures, 64 interp, 24 lexer, 52 parser,
 9 solve, 8 temporal)
```

New tests: two elaboration tests — the `container of (int)` instance
materializing with substituted signatures, its parameterized
`iterator of <type>` reference feeding the worklist into its own
view, and single-instance identity; and `map of (int)` filling
`<value'type>` from its default while `map of (int, string)` stays a
distinct view. One interpreter test — a `box of (int (bits: 8))`
field generating under the view, its method dispatching (payload
doubled), and `is a` accepting the canonical name and the base.

## 2026-11-01 (32) — offline arc: clause 10.2.7 CFS-scoped generation; coverage ranges over expressions

Branch `agent/cfs-gen` (off main after #21 merged). Generation gains
its enclosing scope, and the coverage range parser reaches the corpus
shapes.

- CFS scoping (10.2.7, v1): `gen <obj>` inside a method now gates the
  object with the enclosing instance's constraints that reference it.
  Depth-two paths rebase — `p.v == select { .. }` becomes
  `v == select { .. }` — so the drawn select value narrows the
  generated object's field (the constraint list joins the domains and
  the rejection predicate). Constraints that do not rebase entirely
  (other roots, deeper paths, whole-object equalities) drop rather
  than fail evaluation against the generated frame; full CFS scoping
  is a later milestone. The marquee: two sibling when-view instances
  with opposite distribution policies produce opposite extremes
  (`LO env` → v=0, `HI env` → v=255) from the same `gen p` statement —
  the mini-igen-dist shape.
- Coverage range buckets over expressions (per the igen-dist usage):
  bounds are constant integer expressions (`MAX_UINT-9`,
  `MAX_UINT/20`, `0x21`) evaluated with the sampler's
  choice-expression evaluator; `range([v], ..)` single-value buckets
  have lo == hi; non-string parameters (`10`, `UNDEF`) evaluate with
  the first positive value as the goal, `UNDEF` skipping. With this,
  igen-dist's five address buckets parse fully (`0/5 buckets` in its
  `--coverage` report, note gone). Its samples stay at zero in the
  recorded run because the corpus starts its TCMs from `env.run()`,
  which the interpreter does not auto-invoke — the policies engage
  through the same shape as the new test.

Corpus state (absolute paths): etl 4, cover-edges 4, golden-imports
3, golden-combined/igen-dist/type-min-max/save-restart/d6w 0 —
unchanged; igen-dist runs exit 0.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`,
Windows; no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(173 passed: 14 elab, 1 fixtures, 65 interp, 24 lexer, 52 parser,
 9 solve, 8 temporal)
```

New test: one interpreter test — the two-policy env marquee (a
when-view's `keep soft p.v == select { 1000 : min; }` biases `gen p`
to 0 while its sibling's max policy biases to 255, exercising the
rebase, the domain narrowing and the select draw together). Two
debugging notes for the record: the coverage range rewrite first lost
the closing-bracket advance (caught by the explicit-buckets test),
and the args loop initially accepted only single numeric tokens
(igen-dist's `MAX_UINT/20` parameter forced the expression evaluator
into the loop).

## 2026-11-01 (33) — offline arc: clause 14.3/14.4 derived coverage items (cross, transition)

Branch `agent/cover-cross-transition` (off main after #22 merged).
The recorded cross/transition entries now collect.

- Derived-item model in `src/coverage.rs`: each group carries
  `CoverDerivedData` entries synthesized from its recorded cross and
  transition declarations, under the prose's default names —
  `transition__item` (14.4) and `cross_a__b` (14.3, double
  underscores).
- Transition (14.4): "if item-name had n samples during the test,
  then the transition item has n-1 samples, where each sample has the
  format previous-value, value" — each sampling event records the
  base item's previous→current bucket-label pair (`low→high`); a
  ranged base grades over its n² ordered bucket pairs, an unbucketed
  base counts pairs without a grade.
- Cross (14.3): "shows every combination of values of the first and
  second items" — each sampling event records the tuple of the
  crossed items' current bucket labels (`(low, TRUE)`), graded over
  the cross product of the sources' bucket counts; any unbucketed
  source leaves it ungraded.
- Both join the group's grade average, appear in the `--coverage`
  report with their combinations and hits, and the golden's shapes
  (`transition fsm;`, `cross opcode, carry;`) render as derived items.
- Honest v1 limits: per-instance coverage (14.2.1) and the option
  tables remain recorded, not collected; the transition's n² total is
  the v1 bucket-pair reading (Table 44 is an image in the local
  copy); nested crosses (crossing a cross) flatten to their written
  source names.

Corpus state (absolute paths): etl 4, cover-edges 4, golden-imports
3, golden-combined/igen-dist/type-min-max/save-restart/d6w 0 —
unchanged; golden-combined's `--coverage` renders
`cross_opcode__carry` and `transition__fsm` with honest no-grade
zeros (its sampling events never fire in the recorded run).

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`,
Windows; no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(176 passed: 14 elab, 1 fixtures, 68 interp, 24 lexer, 52 parser,
 9 solve, 8 temporal)
```

New tests: three interpreter tests — a transition over a ranged item
(3 base samples → 2 pairs, `low→high`/`high→low` of 4 ordered pairs,
group grade = average of the item's 100% and the transition's 50%); a
cross over a ranged item and a boolean (2 of the 4 combinations
observed); and a transition over an unbucketed string item counting
one pair without a grade.

## 2026-11-02 (34) — offline arc: clause 15 simple macro expansion

Branch `agent/macro-expansion` (off main after #23 merged). Simple
define-as macros expand before parsing: `MIN_VALUE(addr_t)`-style
invocation sites become their body text.

- New `src/macros.rs`, a pre-pass over the token stream wired into
  both `parse` and `parse_unchecked` (the CLI and the tests take the
  same path):
  - Pattern compilation (15.4): the recorded pattern strings compile
    to term trees — escaped literal terminals (each escaped character
    its own literal, since `\(` lexes as the token `(`), plain literal
    runs (`MIN_VALUE` matches one identifier token), `<[tag']type>`
    non-terminals, `[ ]` option (whitespace-only literals vanish —
    tokenization ignores whitespace, so the corpus's `MIN_VALUE[ ]\(`
    optional-space form is vacuous), `( )` grouping and `|`
    alternation.
  - Matching (15.5.2): recursive descent with backtracking —
    non-terminals try every split length longest-first, bounded by
    bracket depth and depth-0 separators (`,`/`;`), so
    `MIN_VALUE(uint (bits : 8))`-shaped arguments consume whole while
    `f(a, b)` args keep their commas. Budget-capped.
  - Expansion (15.6): the body's `<name>` / `<name'category>`
    references resolve by tag, else by kind when unambiguous, and the
    matched subsequence splices in; the body's trailing `;` belongs to
    the body segment (15.4.5's sequencing rule), not the expansion —
    the invocation site carries its own. Later definitions have
    higher priority (15.5.1); bodies may invoke other macros (the
    pass iterates, definitions re-scoped each round); definitions
    themselves are never rewritten.
- Honest v1 limits (documented): computed macros (15.3) stay
  recorded, not computed — the matcher skips them, so a file's simple
  definition of the same pattern applies (the type-min-max corpus's
  `reject_match()` DAC dance is not modeled); the `<type, ...>`
  repetition form matches as one non-terminal; argument categories
  are not validated; patterns not starting with a literal are inert
  (no anchor).
- The type-min-max corpus (defines only) is untouched by the pass;
  its sibling example file (`type_min_max_usage_ex.e`, real
  `MIN_VALUE(addr_t)` invocations) still reports its pre-existing
  diagnostics — it imports a definitions file the CLI resolves
  beside it, and the full expansion chain there needs the
  `set_of_values` reflection routines (Part 2 library), which remain
  unimplemented.

Debugging notes for the record: the first matcher consumed
non-terminals greedily to the END of the token stream (the anchor
then matched the file's last `)` — caught by the composed-expression
test); the escape handling first merged `\(\)` into one literal `"()"`
(no token ever equals it — caught by the priority test); and the pass
was initially wired only into `parse_unchecked`, so the tests (via
`parse`) saw no expansion while the CLI did — both entries now share
it.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`,
Windows; no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(181 passed: 14 elab, 1 fixtures, 73 interp, 24 lexer, 52 parser,
 9 solve, 8 temporal)
```

New tests: 5 interpreter tests — simple substitution with the
argument reaching the helper; the corpus composition shape
(`MAX_VALUE(t) - MIN_VALUE(t)` expanding independently inside one
expression); an argument with internal parentheses consumed whole;
15.5.1 priority (later same-pattern definition wins); and a body
invoking another macro (15.5.2's re-testing of generated code).

## 2026-11-02 (35) — offline arc: Part 2 reflection routines (set_of_values, 27.4 set pseudo-methods)

Branch `agent/reflection-routines` (off main after #24 merged). The
reflection subset of the Part 2 routine library evaluates — the set
type, its constructor, and its pseudo-methods.

- `Value::Set` (5.1.10): inclusive intervals in canonical form —
  sorted, overlapping and neighboring merged, empties dropped.
  `format_value` renders the canonical form (`[-128..127]`,
  single-value intervals as bare values: `[1..5,10]`).
- `set_of_values(<type>)` / `full_set_of_values(<type>)` (5.8.4/5.8.5):
  a new `Expr::SetOfValues` whose argument PARSES AS A TYPE in
  `parse_primary` — scalar types from width and signedness
  (`uint (bits : 4)` → `[0..15]`), clause-5 `type` declarations from
  their underlying representation, enumeral types as `[0..count-1]`.
  The two routines build the same set today: no range modifiers are
  recorded on declared types, which is 5.8.5's "for types that have
  no range restrictions..." case.
- The 27.4 set pseudo-methods run on set values through the same
  dispatch as list pseudo-methods: `size`/`uint_size` (27.4.4/5),
  `min`/`uint_min`/`int_min` and `max`/`uint_max`/`int_max`
  (27.4.7-27.4.10 — the bottom/top boundary; empty sets error per the
  prose's exception), the `size_is_uint`/`min_is_uint` guards, the
  algebra `union`/`intersect`/`diff` (27.4.1-3, canonicalized), and
  `to_string()`.
- `to_string()` also renders Int/Bool/Enum/Str values (the DAC macro
  shapes: `set.min().to_string()`), and `as_a` to an enumeral type
  maps the integer index to the enumerand (5.8.1) —
  `2.as_a(color_t)`-shaped casts (via a variable: the lexer reads
  `2.as_a` as the real `2.`).
- The set type name is predeclared in type positions
  (`var s: set`, BUILTIN_TYPES) with the empty set default (5.1.10's
  `[]`).
- Honest v1 limits: set literals in expressions (`[-5..-1]`), range
  modifiers on declared types (`type x : int[1..10]` — so
  `full_set_of_values` cannot yet differ from `set_of_values`), bare
  scalar keywords as type arguments (`set_of_values(int)` in
  expression position — needs type objects), and the rf_* reflection
  layer (`rf_manager`, `rf_numeric`) are not modeled. The
  type_min_max example file's remaining diagnostics are exactly its
  bare-scalar invocations (`print MIN_VALUE(int)`) plus the
  pre-existing import-list form.

Corpus state (absolute paths): etl 4, cover-edges 4, golden-imports
3, golden-combined/igen-dist/type-min-max/save-restart/d6w 0 —
unchanged; type-min-max runs exit 0.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`,
Windows; no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(186 passed: 14 elab, 1 fixtures, 78 interp, 24 lexer, 52 parser,
 9 solve, 8 temporal)
```

New tests: 5 interpreter tests — set construction from scalar and
enumeral types (canonical rendering); the 27.4 queries
(size/min/max/uint_size on `[-128..127]`); the algebra with
canonicalization (`bit ∪ nibble = [0..15]`, `bit ∩ nibble = [0..1]`,
`nibble − bit = [2..15]`); the type_min_max chain shape
(`uint_min`/`uint_max`/`uint_size`/`to_string` over a uint-typed
set); and the int→enumeral `as_a` index mapping.

## 2026-11-02 (36) — offline arc: clause 11.3 on event-port members

Branch `agent/on-event-port` (off main after #25 merged). Event ports
and their `on` members parse, compose and run.

- Event-port declarations (9.4): `name: <dir> event_port [is
  instance];` — the port grammar accepts `event_port` alongside
  `simple_port of <type>`, sharing the in/out/inout direction
  spellings; the element type records as `event`.
- `on <port>$ { actions };` members (11.3): the `$` access operator
  (9.4.1) names the event associated with the port, and the lexer
  folds it into the identifier (`p$` is one token) — so the member
  flows through the existing clause 11.2 machinery unchanged: dotted
  paths compose base-first then extends (repeated declarations extend
  per 11.3's "same effect as using is also"), and `emit p$;` triggers
  the port, running the blocks immediately before any waiting TCM
  (trivially true in the inline-execution model).
- The 11.3 restriction is enforced: "The on action block shall not
  contain any time-consuming actions or TCMs" — `wait`, temporal
  `wait`, `sync`, `start` and `compute` at any depth of an `on` body
  (if/while/case/for included) report `E0210` at check time. The
  restriction immediately caught a pre-existing test that violated it
  (an on block carrying a `wait` to advance the cycle — entry 26);
  that test now uses a started TCM, which is the legal shape.
- Honest v1 limits: the trigger side is emit-only — HDL edge- and
  bind()-driven port triggering rides the `@sim` gap (9.4's
  bind()/hdl_path()/edge() attributes are not modeled); the 11.3
  syntax example is an image in the local redline copy, so the port
  declaration spelling follows the tokens real files use
  (`event_port`) rather than a guessed figure.

Corpus state (absolute paths): etl 4, cover-edges 4, golden-imports
3, golden-combined/igen-dist/type-min-max/save-restart/d6w 0 —
unchanged.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`,
Windows; no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(188 passed: 14 elab, 1 fixtures, 80 interp, 24 lexer, 52 parser,
 9 solve, 8 temporal)
```

New tests: two interpreter tests — `on p$` with repeated declarations
both running on `emit p$`, and the E0210 restriction over `wait` and
`start` in on bodies; the temporal-wait fixture extends with the
event-port declaration, an `on rst$` block and `emit rst$`.

## 2026-11-03 (37) — offline arc: clause 14.2.1 per-instance coverage; option table readings

Branch `agent/per-instance-coverage` (off main after #26 merged). The
per_instance option, the option-table readings and per-instance
grading land.

- Option scanning from the recorded tokens (Tables 40/41 are images
  in the local redline copy; the common readings apply, documented):
  `at_least = N` sets the per-bucket goal (both explicit `ranges`
  buckets and the 14.2 defaults), `per_instance = TRUE` marks the
  item per-instance — the item-level spelling in `using` and the
  group-level spelling as a group option entry (which marks every
  item) — and `weight = N` scales the item's grade contribution.
- `CoverInstanceData` (14.2.1): "an instance is created for every
  valid bucket of the per_instance item. Any instance that is not
  sampled is marked as a hole" — instance name is
  `item==bucket_label`, grade 1.0 when the bucket's hits reach its
  goal, 0.0 otherwise, holes = zero hits.
- Group grading (14.2.1's weighted reading): with per_instance items,
  the group grade is the weighted grades of all the instances plus
  the per_type bucket (14.2.1's "sum of all valid buckets for all the
  per_instance items +1 (the per_type bucket)") — one unit whose
  grade is the average of the per_type items' grades; without them,
  the weighted average of the gradeable items' grades (weight 1 when
  the option is absent). The per_type item still collects its
  accumulated data (14.2.1's "as if it were not a per_instance
  item").
- `run --coverage` lists each per_instance item's instances with
  hits/goal/grade and `[hole]` marks.
- Honest v1 limits: instance naming uses the bucket label as written
  (integer instances use the decimal radix per 14.2.1 — the labels
  come from the ranges clauses); `weight` applies in the group grade
  only (no aggregation hierarchy above the group); per-instance items
  participating in crosses/transitions are excluded from those
  instances (the exclusion falls out of the derived items' sources
  reading); per-unit-instance options (14.1.1) are not modeled.

Corpus state (absolute paths): etl 4, cover-edges 4, golden-imports
3, golden-combined/igen-dist/type-min-max/save-restart/d6w 0 —
unchanged.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`,
Windows; no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(192 passed: 14 elab, 1 fixtures, 84 interp, 24 lexer, 52 parser,
 9 solve, 8 temporal)
```

New tests: 4 interpreter tests — per-bucket instances with the
`item==bucket` names and a 100% group grade; unsampled buckets as
holes pulling the group grade to (1.0 + 1/3)/4; `at_least = 2` as the
bucket goal with the weighted group grade (1.0 + 0.5×3)/4; and the
group-level `per_instance = TRUE` spelling marking all items.

## 2026-11-03 (38) — offline arc: clause 20 repeat until + the do sugar

Branch `agent/repeat-statement` (off main after #27 merged). Clause
20's remaining statement forms parse and run.

- `repeat [do] { body } until cond;` (20.2.2): a new `Stmt::RepeatUntil`
  — "the action block is executed at least once" (the NOTE 1 do-while
  order): the body runs, then the condition is checked; `break`
  exits, `continue` restarts. The condition must be boolean
  (`E0402`); a missing `until` after the body reports `E0112` (the
  recorded unsupported-statements fixture updated: `repeat` without
  `until` is now a malformed loop, not an unsupported statement).
- The optional `do` sugar (20.2's NOTE 2 — "purely syntactic sugar"):
  `while cond do { .. };` and `if cond do { .. };` accept the keyword,
  and `if cond do <one statement>;` parses the single-statement form
  — the inner statement consumes its own `;`, so `parse_if_inner`
  returns a needs-terminator flag instead of the caller adding one
  unconditionally (else-if chains propagate it).
- Honest v1 limits: `repeat until` inside a coroutine suspends like a
  `while` (the WhileLoop resume context), but the one-statement `do`
  forms inside `if` bodies keep the inline spin (the documented
  cursor-reach limit).
- Debugging note for the record: the first repeat tests hung —
  sys's uint fields are implicitly randomized before `run()` (the M2
  implicit-gen behavior), so `until i == 3` over a FIELD never
  terminates; the tests now use locals, matching the existing while
  tests' shape.

Corpus state (absolute paths): etl 4, cover-edges 4, golden-imports
3, golden-combined/igen-dist/type-min-max/save-restart/d6w 0 —
unchanged.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`,
Windows; no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(197 passed: 14 elab, 1 fixtures, 89 interp, 24 lexer, 52 parser,
 9 solve, 8 temporal)
```

New tests: 5 interpreter tests — repeat until counting to 3; the
do-while order (an already-TRUE condition still runs the body once);
`break` exiting a repeat loop; the one-statement `if cond do
<statement>;` form (true and false branches); and the `while cond do
{ .. };` sugar.

## 2026-11-03 (39) — offline arc: clause 21 `#define` replacement rules and `#else`

Branch `agent/define-else` (off main after #28 merged). The
preprocessor's remaining halves land: clause 21.3 replacement rules
and 21.2's `#else`.

- Standard-source note: the local redline PDF renders the 21.2/21.3
  syntax examples as images (confirmed with pdftotext — the prose
  extracts, the example blocks do not). Only the PROSE was
  implemented: 21.2's "the optional #else syntax provides an
  alternative statement when the condition does not hold" with the
  primary code "inside the (first) braces", and 21.3's "the given
  text is substituted for every occurrence of the name in subsequent
  code, except inside string literals" / "applies only to code that
  comes later". No `#endif` appears anywhere in the extractable text,
  so none is accepted — a stray `#endif` reports `E0117`.
- `#define` replacement rules (`src/preproc.rs`, new): a token-level
  pre-pass before the clause 15 macro expansion (Annex B.5.1's two
  phases: preprocessor directives execute first). It walks the stream
  in source order keeping the rules defined so far, splices each
  defined name's replacement tokens (the span
  `Parser::consume_hash_replacement` consumes: to `;` or the name's
  line end) in place of the identifier, skips the
  `#define`/`#undef` directives themselves and string literals (they
  are single tokens), and carries excluded `#ifdef`/`#ifndef`
  branches through untouched — a `#define` inside an excluded branch
  defines nothing. `#undef` removes the rule from that point onward.
- `#else` (21.2) parses at statement, struct-member and action
  positions: the failing side's group is skipped without parsing, the
  winning side parses through the same position-dependent grammar.
  Both `} #else { .. };` and `}; #else { .. };` spellings are
  accepted — the prose does not fix the terminator's position (the
  syntax example is an image), so both the parser and the pre-pass
  tolerate the `;` between the groups.
- Honest v1 limits, documented in the module: substitution is
  single-pass (a name that only appears inside another rule's
  replacement stays as written; 21.3's "the replacement shall not
  contain the name" runtime error cannot loop and is not diagnosed);
  redefinition takes the last value; the backtick-prefixed
  Verilog-style preprocessor names of 4.1.5.4 are not accepted;
  cross-file visibility (B.5's include order through `import`) is not
  modeled — each file preprocesses independently; substituted tokens
  keep their `#define`-line spans, so a diagnostic inside substituted
  text points at the definition (the clause 15 macro pre-pass does
  the same); the parser's `defined` flag set and the pre-pass's rule
  map update on the same directives in the same order (directive
  headers are never substituted).

Corpus state (absolute paths): etl 4 (the same four `E0203
base_struct` lines), cover-edges 4, golden-imports 3,
golden-combined/igen-dist/type-min-max/save-restart/d6w 0 —
unchanged. The etl corpus's `#define INCREASE_SIZE 2` /
`#define REDUCE_SIZE 4` sites (`size * REDUCE_SIZE`,
`tmp.resize(size * INCREASE_SIZE)`) now genuinely substitute.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`,
Windows; no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(206 passed: 14 elab, 1 fixtures, 92 interp, 24 lexer, 58 parser,
 9 solve, 8 temporal)
```

New tests: 7 golden fixtures — replacement as a type (`#define T
int` + `f: T;`), substitution reaching the well-formedness pass
(`#define N 0` + `uint(bits: N)` → `E0205` at the definition-side
span), a `#define` inside an excluded branch registering nothing,
`#undef` stopping the replacement (`E0203` at the use site), and
`#else` at member/action positions in both directions; and 9 unit
tests — 6 parser (substitution into a var initializer, the
single-pass limit, directives and string literals untouched,
`#else` branch selection at member and action levels in both
spellings, and the excluded-branch registration guard) and 3
interpreter (substitution visible at run time, string literals
untouched, and both `#else` branches running).
## 2026-11-03 (40) — offline arc: clause 6.6 when-chaining (chained modifiers, nested when-in-when)

Branch `agent/when-chaining` (off main after #28 merged). Chained
modifiers and nested when-in-when blocks compose: a when block is no
longer limited to one modifier, and a `when` member inside a when
block's members applies on top of the enclosing view.

Prose findings that constrained the design (clause references only,
local copy under `local/`):

- 6.6.1's subtype examples include combinations of fields (a subtype
  qualified by one field's value AND another's), and Annex C.2.2
  states that with when inheritance all combinations of the
  determinant dimensions exist implicitly and are enumerated only
  when something is said about them. So multiple modifiers in one
  block read CONJUNCTIVELY: the subtype requires every determinant
  field to hold its value. The 30.2.4.5 reflection prose agrees: a
  subtype's determinant FIELDS are a list (`get_determinant_fields`)
  and the canonical name joins the qualifying values before the base
  type name — matching this repo's space-joined `V1 V2 base` spelling
  (the reference-grammar `value'id` pairing stays our v1 reading of
  which field each value belongs to).
- Nested when-in-when is NOT named in the 6.6/6.6.1 prose (its syntax
  examples are figures, not extractable text). The containment
  reading is supported by 30.2.4.5's `get_contributors` — "all the
  explicit subtypes where this subtype is contained" — so the
  nested-when semantics here (inner modifiers run against the outer
  synthesized view's fields, producing a further combined view whose
  base chain walks the outer view) are the v1 reading, documented as
  such.

Implementation:

- Elaboration (`src/elab.rs`): `when_of` is now a
  `Vec<(String, String)>` — every (field id, value) pair a block's
  modifiers declare. Two spellings pair: any number of `value'id`
  modifiers (each names its determinant field), and the corpus's
  two-bare-modifier space form (value first, field id second —
  unchanged). A lone bare modifier still synthesizes nothing.
  Synthesis is a worklist seeded with every type: each block
  synthesizes a view named with its values space-joined before the
  ROOT base type's name (`LO HI pkt`), the combined pairs, and base =
  the view it applies on top of (`LO HI pkt` → `LO pkt` → `pkt`), so
  `is a` walks the containment chain. Nested `Member::When` members
  are carried into the view's whens and reprocessed until no new
  views appear (budget 200). A view carries only the when-in-when
  members of the block it came from — the base's sibling when blocks
  are alternative subtypes, not refinements (re-applying them had
  cascaded contradictory views before that rule).
- Parser (`src/parser.rs`): `parse_type`'s when-qualified form now
  accepts two or more identifiers (`f: LO HI pkt`), parsed as NESTED
  `TypeExpr::WhenQualified` (value `LO`, base the rest) — the AST
  shape that keeps `render`, `check`, template substitution and the
  interpreter recursions composing naturally; the last identifier is
  the base, the rest nest right-to-left.
- Interpreter (`src/interp.rs`): `when_view_name` flattens a nested
  when-qualified chain to its combined view name (`LO HI pkt`);
  `default_for`, `sample_type`/gen and the `is a` type match resolve
  the combined name. `is a` needs no change: it walks the declared
  base chain, now including the view-to-view hop.

Known v1 limits (documented, unchanged where noted): a `'` followed
by b/o/h/d/x still lexes as a based literal, so a `value'id`-spelled
modifier whose field id starts with one of those letters (e.g.
`HI'dir`) does not parse — the space form (`when HI dir`) avoids it;
the determinant pairs are not re-verified against the base's fields
or enumeral sets; base-typed instances still do not dynamically match
subtypes; sibling when blocks are treated as mutually exclusive
alternatives (their members never merge into one view).

Corpus state (absolute paths): etl 4, cover-edges 4, golden-imports
3, golden-combined/igen-dist/type-min-max/save-restart/d6w 0 —
unchanged; the igen-dist and golden-combined `run`s keep exit 0 with
byte-identical output to the pre-change build.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`,
Windows; no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(200 passed: 16 elab, 1 fixtures, 90 interp, 24 lexer, 52 parser,
 9 solve, 8 temporal)
```

New tests: two elaboration tests — a chained-modifier block
(`when LO'kind HI'sel`) synthesizing one combined view with both
(field, value) pairs, and a two-level when-in-when composing both
levels' constraints into a `LO HI pkt` view whose base is the outer
view; one interpreter test — a chained-qualified field
(`p: LO HI pkt`) generating under both levels' select policies (one
field to min, the other to max) with `is a` walking the containment
chain to the base type.
## 2026-11-03 (41) — offline arc: clause 15.3 computed macros (parse-time evaluation, reject_match decline)

Branch `agent/computed-macros` (off main after #28 merged). Define-
as-computed bodies now expand instead of being skipped: 15.3 treats
the body as a method returning a string, so `src/macros.rs` gained a
restricted parse-time interpreter.

- Evaluation happens at match time (15.3: the body executes whenever
  the match expression matched), in the same later-first priority
  order as simple definitions (15.5.1) — a computed definition LATER
  than a simple one of the same pattern takes priority, and on
  decline the matcher continues to the next lower-priority
  definition (15.5.2's backtracking), so the type-min-max corpus's
  define-as-computed/define-as pair resolves to the simple
  expansion.
- The interpreter reads a restricted statement subset over the
  recorded body tokens: `var name := expr;` (also `=` and
  `var name: type = expr;` forms), plain assignments, braced
  `if cond { .. } [else { .. }]`, `return expr;`, and expression
  statements. Expressions: string parameters (their matched text as
  a string, 15.3/15.6 — by tag, else by unique kind, both as the
  `<...>` reference spelling and as bare variable names), string
  literals (escape sequences interpreted), the implicit
  TRUE/FALSE booleans, `+` concatenation, `==`/`!=` comparisons,
  parenthesized expressions, and calls — of which only
  `reject_match()` is known.
- Decline paths (all fall through to the next lower-priority
  definition, documented in code): a `reject_match()` action
  executing (the standard's decline mechanism — as a statement or
  inside an evaluated call); any unevaluable construct — unknown
  routines (`rf_manager` first among them: 15.3 says parsing
  precedes semantic analysis, so module definitions cannot be
  presupposed), method-style calls, `is a` checks, `else if`
  chains, list/number expressions, non-string results; a
  non-lexing or empty result (15.3: the returned string must be a
  legal e construct of the category). The body's `result` variable
  or `return` action supplies the expansion string; the expansion
  pass's iteration cap and definitions-never-rewritten rule are
  untouched.
- Honest v1 limits (documented): a computed body that branches on
  reflection cannot succeed at parse time (there are no type
  objects), so the corpus's DAC bodies always decline to their
  simple twins; parameter text is the raw matched source text (a
  quoted-string actual carries its quotes); expansion token spans
  are relative to the returned string, not the original source.

Debugging notes for the record: the definition scanner consumes the
body segment's trailing `;` (15.4.5), so the computed-body parser
lets the last statement end at end-of-tokens (the first cut required
a terminator that had already been consumed — caught by the
parameter-concatenation test); and a body string literal's stored
value keeps escape sequences raw, so `return "out(\"x\")";` emitted
backslashes into the result and broke re-tokenization — string
literal values now interpret escapes (the interp's `unescape`,
shared via `pub(crate)`), while parameter text stays the raw source
form (15.6's string part of the input text).

Corpus state (absolute paths): etl 4, cover-edges 4, golden-imports
3, golden-combined/igen-dist/type-min-max/save-restart/d6w 0 —
unchanged; type-min-max only defines macros and must stay 0.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`,
Windows; no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(201 passed: 14 elab, 1 fixtures, 93 interp, 24 lexer, 52 parser,
 9 solve, 8 temporal)
```

New tests: 4 interpreter tests — a computed macro building its
expansion string from the parameter (later than a same-pattern
simple definition, so the computed one wins per 15.5.1); the
type-min-max DAC shape where the computed body executes
`reject_match()` and the simple definition applies; a computed body
calling an unknown routine declining to the simple definition; and
the implicit `result` variable as the return value.

## 2026-11-04 (42) — offline arc: simulator-side temporal integration (@sim sampling, event-driven TCM wake, port triggering)

Branch `agent/sim-integration` (off main after #31 merged). The three
M1/M3 "not wired" gaps around the simulator side are closed: `@sim`
edge sampling, `@`-triggered TCM invocation, and event-port
triggering on the bound HDL object's edges.

Prose readings this is built on (the redline copy's syntax figures
for 11.3/9.4 stay images; everything below is extractable prose):

- 11.1.4.2: `rise | fall | change (HDL expression) @sim` "causes the
  DUT to be monitored for a change in that expression", and control
  passes to the e program for each delta cycle where a monitored
  expression changes. In the virtual-cycle model every scheduler
  cycle boundary is a sampling point: each instance of the declaring
  type samples its trigger's expression in its own context (11.1.2 —
  every instance owns its event instances), edge-detects against the
  previous sample, and fires through the `emit` machinery (temporal
  log, on blocks, coverage, TCM wake).
- 17.1.2 + 13.1.1 + 13.1.2: a TCM "implicitly synchronize[s] on the
  designated sampling event upon entry, as if the action sync @event
  was added at the top of the TCM code". So a started `tcm() @ev`
  commences when `ev` occurs (17.2.1: "when its sampling event is
  emitted"), not one cycle after `start` — unless `ev` already
  occurred this tick ("or immediately if the sampling event has
  already been emitted for the current tick"), when the entry sync
  succeeds in the current cycle. `@sys.any` — the stand-alone clock
  that "is the only one that occurs automatically" (11.1.4.1.1) — and
  `@sim`/`@sys.new_time` keep the every-cycle implicit sync cycle.
- 9.4 + 9.7.2.9: "An external port needs to have an hdl_path()
  attribute to specify the name of the object to which it is
  connected" — the existing `keep p.hdl_path() == "…"` resolution is
  the binding, and the port's event fires on the bound object's edge
  per 9.7.2.8's `edge()` attribute (prose-extractable: values
  change/rise/fall — "equivalent to the behavior of @sim TEs" — and
  any_change, default change), read from `keep p.edge() == <value>`
  constraints.

Implementation shape:

- `hdl::SignalSource` (`Real(HdlSession)` or
  `Injected(InjectedSignals)`): the interpreter's signal seam. The
  injected form is a sparse per-virtual-cycle history
  (`interp::run_with_signals`) so tests and offline runs drive `@sim`
  edges deterministically without the container-only xezim binary;
  `hdl_load` replaces it with a real session. The `hdl_*` builtins
  and the quoted-path/port accesses all route through it.
- One firing path (`Interp::fire_event`) shared by `emit` and the
  simulator-side triggers: record in the current cycle's log state,
  wake TCMs parked on the event, run the `on` blocks (11.2 — on
  blocks execute before TCMs waiting for the same event), sample the
  cover groups. The scheduler's nobody-eligible branch samples after
  each cycle advance; cycle 0's sample only establishes the baseline.
- The scheduler's eligibility scan skips coroutines parked on their
  TCM sampling event; a fire wakes them into the current cycle (they
  still only run once the running turn yields — 17.2.2). The
  5,000-cycle budget semantics are unchanged: a parked TCM keeps the
  run alive until its event occurs, and a never-occurring event runs
  to the budget guard (E0409).

Honest v1 limits (documented in the code and ARCHITECTURE): only
single-item `rise/fall/change(HDL expression) @sim` triggers fire
(multi-item triggers, `cycle`, grouped guards and `true(exp)` samples
would need the temporal engine at the sampling point); the first
sample establishes the baseline (no X initial state, so no edge on
it); values are plain integers, so 9.7.2.8's MVL edge spellings fall
back to the `change` default and `any_change` degrades to `change`;
sampling happens at the scheduler's cycle boundaries only (inline
spins do not advance the HDL — the existing limitation); body-level
bare `wait`/`sync` still synchronize on the stand-alone sys.any clock
(the default-sampling-event reading of 13.1.2/13.1.3 is applied to
the entry sync only); the run log stays one global timeline
(occurrences are per-cycle states, not per-instance schedules); the
predefined events still do not occur in the log (sys.any-based
expect rules stay vacuous); `edge()` constraints inside `when` blocks
are not seen (top-level keeps only, like the hdl_path() extraction).

One additive elaboration change rides along: `Member::Event` (dropped
until now) and an event-port flag flow into `ElaboratedType`
(`events`, `PortInfo::is_event`) — the interpreter needs the recorded
triggers; nothing else consumes them.

Corpus state (absolute paths, release build of this branch):
`B:/e1647-rs/local/corpora/etl-simple-examples.e` 4,
`cover-edges.e` 4, `B:/e1647-rs/local/sn-cpu-tutorial/
golden-combined-imports.e` 3, `golden-combined.e`/`igen-dist.e`/
`type-min-max-usage.e`/`save-restart-test1.e`/`d6w/testbench.e` 0 —
unchanged; the d6w `run --seed 1` still ends with the reference
line and exit 0.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`,
Windows; no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(221 passed: 16 elab, 1 fixtures, 105 interp, 24 lexer, 58 parser,
 9 solve, 8 temporal)
```

New tests: 8 interpreter tests — a rise trigger firing exactly on the
rising edges of an injected history; a change trigger firing on every
change (the flat cycle silent); no fire without any HDL source; a
`tcm() @ev` TCM commencing only when ev occurs (output ordering
against a per-cycle watcher); the already-emitted-this-tick entry
(17.2.1 — the TCM runs before the watcher's first cycle); a parked
TCM holding the run to the E0409 budget; a bound event port firing
its `on p$` blocks on the bound object's change edge; and the
`edge() == rise` attribute restricting the port's edges.

## 2026-11-04 (44) — offline arc: clause 28.9 coverage configuration and remaining option tables

Branch `agent/coverage-config` (off main after #31 merged). The 28.9
configuration and the option entries whose semantics the extractable
prose defines land.

Prose findings that constrained the design (clause references only,
local copy under `local/`; the option tables 40/41 are images in the
redline PDF):

- 28.9 is the `set_config()` routine: "This routine sets the
  configuration options to the specified values" — the syntax
  example's `set_config(memory, gc_threshold, 100M)` gives the
  category, option, value shape, and 7.6.1's `set_config_max()`
  variant "sets the numeric options of a particular category to the
  specified maximum values". No option catalog is extractable.
- 14.2: "If ranges is not specified, the default number of buckets is
  16 [set by the max_int_buckets coverage configuration option (see
  28.9)]", and both 14.1 and 14.2 carry the NOTE "Unless coverage mode
  is turned on first (see 28.9), no coverage results are collected,
  even if cover groups and cover items are defined" — these two are
  the extractable coverage configuration.
- 14.1.1/14.2.3: "if one or both of no_collect and
  instance_no_collect options of the group [item] are evaluated to
  TRUE, the group [item] shall not be presented under that instance"
  — the OR relation is extractable prose; the option definitions and
  the `per_unit_instance` setting itself live in Table 40 (an image),
  and the bool-exp form's implicit `inst` variable needs the
  per-unit-instance model this repo does not have.
- 14.3/14.4: a cross or transition "creates a new item with a name
  specified using a name option or by using the default name" — the
  `name` option is extractable prose for both derived kinds.
- 14.8.2/14.8.3: weights and the at_least minimum "can be specified
  in the coverage group or item definitions" — the group-level
  spellings of the two options already applied at item level.
- Not extractable: `radix` appears only in a 14.6 syntax example
  (`using also radix = HEX`); `when`/`never`/`ignore`/`illegal` have
  no extractable semantics — documented gaps, not implemented.

Implementation (all in `src/coverage.rs`; the interpreter's call site
is untouched):

- `CoverageConfig` (28.9): `mode` (14.1's NOTE — with the mode off
  nothing is built, so the interpreter finds no groups to sample and
  the report presents none) and `max_int_buckets` (14.2's bracket —
  the integer items' default bucket count; 0 leaves the item
  ungradeable with a note). `build()` keeps the default
  configuration; `build_with_config` is the documented API. The
  default keeps collection ON — a documented deviation: the
  standard's mode starts off until `set_config` turns it on, and
  that routine is not wired yet.
- `no_collect`/`instance_no_collect` (group option entries and item
  `using` spellings): the constant TRUE spelling drops the group/item
  from the collected model — not sampled, not graded, not presented;
  the OR relation reads every declaration of the group.
- The cross/transition `name = "..."` option replaces the default
  `cross_a__b`/`transition__base` names.
- Group-level `at_least`/`weight` option entries apply to every item
  of the group that does not set its own; the item-level spelling
  wins (the v1 reading of "set in the coverage group or item
  definitions" — the procedural `covers.set_*()` overrides of 14.8
  are not wired).

Honest v1 limits: the `set_config()`/`set_config_max()` routines are
not wired (the routine library owns the interpreter's routine table);
the bool-exp option forms, the implicit `inst` variable and the
`per_unit_instance` scoping are not modeled; `instance_ignore`/
`instance_illegal`, the `radix`/`when`/`never` options and item
`using also` extension overrides (14.6) remain unmodeled; a
group-level `weight` has no level above the group to scale (no
type-level grade exists in the model).

Corpus state (absolute paths, run --seed 1): etl 4, cover-edges 4,
golden-imports 3, golden-combined/igen-dist/type-min-max/
save-restart/d6w 0 — unchanged; golden-combined and igen-dist keep
exit 0, and the `--coverage` reports of cover-edges, golden-combined
and igen-dist are byte-identical to the pre-change main build.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`,
Windows; no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(220 passed: 16 elab, 1 fixtures, 104 interp, 24 lexer, 58 parser,
 9 solve, 8 temporal)
```

New tests: 7 interpreter tests — the coverage mode off building
nothing (and the default keeping collection on, per the documented
deviation); `max_int_buckets` at 4 and at 0 (no default buckets, the
honest note); group-level `no_collect = TRUE` and the OR relation's
`instance_no_collect = TRUE` suppressing the group; the item-level
`no_collect` dropping the item while a sibling collects; the
cross/transition `name` option; and the group-level
`at_least`/`weight` applying where the item sets none (the item-level
`at_least` winning).

## 2026-11-04 (43) — offline arc: clause 8.2 template extensions and method-body instance substitution

Branch `agent/template-extend` (off main after #32 merged). Template
extensions stop being unparsed, and the substitution reaches into
method bodies.

Prose findings that constrained the design (clause references only,
local copy under `local/`; the 8.1.2 category table, the 8.2 syntax
examples and the 8.2.1 example bodies are images in the redline PDF):

- 8.2: "The number of parameters in param-list must be equal to the
  number of parameters in the original template declaration. Tags may
  be different from the original template declaration" — an
  extension's formals bind positionally under its OWN tags; a count
  mismatch is a definition error.
- 8.2: "A template extension applies to a given template instance if
  and only if all type parameters comply with these specifications:
  Category compliance where type-category is specified, or the
  specific type where type-name is specified" — the parameter
  specification is either a category or a concrete type name.
- 8.3's NOTE 3: "each one of them applies to this template instance
  only if all actual parameters comply with types or type categories
  specified for the corresponding parameters in that extension", and
  8.2's example prose (the extension "applies only to those instances
  ... whose first parameter is an enumerated type ... It does not
  apply to instances whose first parameter is a different type") —
  non-compliance means the extension does not apply at all.
- 8.3's prose names the `scalar` category (a struct actual "does not
  comply with the category specified for the first parameter--scalar")
  and 5.1's prose reads scalar types as "numeric, Boolean, or
  enumerated" (so an enumerated type complies with `scalar`); 8.2.1's
  prose gives the `enumerated` reading. The full category table of
  8.1.2 is an image, so only `type` (no constraint), `scalar` and
  `enumerated` are honored — any other spelling reads as the
  specific-type form, which can only ever mismatch (never a false
  application).
- 8.3: "substituting an actual parameter for the corresponding
  template parameter within the template body, including within the
  bodies of any applicable template extensions" — the substitution
  covers method bodies, both for the template's own body and for a
  complying extension's members.

Implementation:

- Parser (`src/parser.rs`, `src/ast.rs`): `template extend name of
  <params> { .. };` parses as a `Declaration` with kind `Extend` and
  the new `template_extend` flag, carrying the extension's own
  `TemplateParam` list (tags and categories recorded); the
  `like` clause stays a template-definition form and is not parsed on
  extensions (8.2's syntax image).
- Elaboration (`src/elab.rs`): template extensions are indexed
  separately from plain extends — a template's own composed members
  stay extension-free. The monomorphization worklist applies, per
  materialized instance: the 8.2 arity check (`E0304`, once per
  extension, the extension skipped); 8.3-NOTE-3 compliance (each
  actual against its position's specification — `type`/bare tag: no
  constraint, `scalar`/`enumerated` shape checks over the elaborated
  types, any other spelling: the actual must render to exactly that
  name); then a complying extension's fields, constraints, methods
  and `on` blocks substitute the extension's formals with the
  instance's actuals (defaults filled, positional) and append. The
  original template's own parameter categories stay unvalidated
  (8.3's NOTE 2), consistent with their treatment before extensions
  landed.
- Method-body instance substitution (8.3): a small walker
  substitutes the formals in every type position inside method
  bodies — local-variable types, `new <type>`, `as_a(<type>)`,
  `is [not] a <type>`, `set_of_values(<type>)` — recursively through
  statements and expressions, in both template bodies and extension
  members; every template reference the substitution uncovers joins
  the worklist, so a body's `new iterator of <type>` materializes
  that instance even when no field or signature names it (the
  worklist's initial seeding walks bodies too, for non-template
  types' bodies).
- Interpreter (`src/interp.rs`, minimal — the evaluator's `Expr::New`
  arm only): typed `new <named-or-template-instance> [with { .. }]`
  (6.2) instantiates the named type (the monomorphized instance's
  canonical name for a template reference) and runs the `with`
  assignments against the fresh instance; untyped `new` and other
  type spellings keep the explicit `E0402` not-supported diagnostic.

Honest v1 limits (documented in code and the docs): 8.4's template
when-subtype instances do not compose; extension members beyond
fields/constraints/methods/`on` blocks (whens, covers, expects,
ports, events) do not compose; the 8.2 subtype propagation of applied
extensions ("as well as to all subtypes of those instances") has no
surface yet (template instances do not synthesize when views); an
extension method overriding a template method without `is also` is
not an elaboration error (E0303 runs per declaration, not per
instance — both method blocks would run); untyped `new` stays
unsupported.

Corpus state (absolute paths, run --seed 1): etl 4, cover-edges 4,
golden-imports 3, golden-combined/igen-dist/type-min-max/
save-restart/d6w 0 — unchanged. Run outputs of all eight corpus
files are byte-identical to a main-branch build (`git stash`
rebuild), as is the etl `elab` diagnostic set (35 pre-existing
E0303s); golden-combined keeps exit 0 with the pre-existing
implicit-gen E0407.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`,
Windows; no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(225 passed: 20 elab, 1 fixtures, 105 interp, 24 lexer, 58 parser,
 9 solve, 8 temporal)
```

New tests: 4 elaboration tests — the enumerated-category extension
applying to `packet of (color_e, int)` (its field and method on the
view, the actual substituted through the extension's own tags) and
not applying to `packet of (int, int)`; the specific-type parameter
form applying only to the exactly-named actual; the 8.2 arity
mismatch reporting `E0304` once while the instance still
materializes; and a body-only `new iterator of <type>` reference
materializing `iterator of (int)` with the body's `new` substituted.
1 interpreter test — a template method whose body does
`new iterator of <type> with { idx = 41; }` returning a usable
instance across the chain sys → `container of (int)` →
`iterator of (int)` (the instance referenced only in the body; field
read and method dispatch on the returned object).

## 2026-11-05 (47) — offline arc: multi-simulator HDL abstraction and the wasm32 target

Branch `agent/hdl-abstraction` (off main after #34 merged). Two M4 items
in one arc: the simulator backend seam (hdl.rs is no longer "one
simulator binding" structurally) and the wasm32 compilation check.

No standard-text findings constrained this arc — it is structural work
on the bridge side; no clause of IEEE 1647 names a simulator API (the
standard's procedural interface concerns are the e side's
`hdl_path()`/port bindings already covered by earlier entries). The
VHPI shape is named only as the documented example of a future third
backend; it is not implemented.

Implementation:

- `src/hdl.rs` — the seam. `Simulator` is the backend contract:
  `load(design_file, top)` (sized-only — it constructs the session),
  `name()`, `set`, `get`, `run`, `finish` (the object methods dispatch
  through `dyn Simulator`). Registered explicitly:
  `registered_backends()` / `default_backend()` / `load_backend(name,
  ..)` / `load(design_file, top)` — the last is the constructor the
  interpreter's `hdl_load` resolves through. Two backends:
  `XezimSimulator` (native targets; delegates 1:1 to the unchanged
  `HdlSession`, which keeps its public surface and its allowlist/argv
  security posture) and `NullSimulator` (every operation reports the
  honest no-simulator error, `load` fails up front, `finish` is a
  no-op). `SignalSource::Real` now holds `Box<dyn Simulator>` — the
  interpreter's other call sites and the injected seam are untouched
  (semantics unchanged; interp.rs changed by one hunk, the `hdl_load`
  constructor call).
- wasm32 posture: the whole toolchain probe showed **nothing blocks
  wasm32 compilation** — `std::process` types compile for
  `wasm32-unknown-unknown` (a spawn would only fail at runtime), and
  `cargo check --target wasm32-unknown-unknown` passed on the
  pre-change branch already. The cfg gate (`not(target_arch =
  "wasm32")` around `valid_location`/`HdlSession`/`XezimSimulator`) is
  therefore the honest *runtime* posture, not a compile fix: under
  wasm32 the spawning backend is not compiled at all, the null backend
  is the only registered one, and `hdl_load` there fails up front with
  the no-simulator error instead of a misleading PATH/container spawn
  message. Native defaults are unchanged (default backend is still
  xezim; identical error strings through the delegation).
- `.github/workflows/ci.yml` — `check-wasm` job on ubuntu:
  `rust-toolchain` with `targets: wasm32-unknown-unknown` (the rustup
  target add) and `cargo check --target wasm32-unknown-unknown` plus
  the `--all-targets` form. Compilation check only — no wasm runtime
  test, no browser sandbox API.
- Docs: ROADMAP M4 (the "one simulator binding, not a multi-backend
  abstract API" wording replaced by the seam description; the WASM item
  moved from `[ ]` to `[~]` with what actually lands) and ARCHITECTURE
  (the hdl.rs bullet, the sketch line, the still-ahead list and the
  not-implemented list shrunk to what remains: bindings beyond xezim,
  a backend selection knob, a browser sandbox API).

Honest v1 limits: no VHPI (the registry's `vhpi` arm is deliberately
absent — the unknown-name test exercises exactly that answer); no
backend selection knob (`hdl_load` always resolves the default backend;
`load_backend` is the explicit form for tests and future
configuration); the `XezimSimulator` delegation is name-and-registration
asserted only — spawning is exercised solely by the container's
`compose mixed` service (unchanged), since `xezim` is unavailable
outside it; the wasm check is compile-only.

Corpus state (absolute paths, release build of this branch, parse
diagnostic counts): `B:/e1647-rs/local/corpora/etl-simple-examples.e` 4,
`B:/e1647-rs/local/corpora/cover-edges.e` 4, `B:/e1647-rs/local/
sn-cpu-tutorial/golden-combined-imports.e` 3, `golden-combined.e`
/ `igen-dist.e` / `type-min-max-usage.e` / `save-restart-test1.e` /
`d6w/testbench.e` 0 — unchanged; the d6w `run --seed 1` still ends with
the reference line and exit 0.

Commands and results (toolchain `1.99.1-x86_64-pc-windows-gnu`,
Windows; no solver-touching changes, CI verifies the feature build):

```
$ cargo fmt --all -- --check
(pass)
$ cargo clippy --all-targets -- -D warnings
(pass, no warnings)
$ cargo test --all-targets
(240 passed: 7 hdl (lib unittests), 20 elab, 1 fixtures, 113 interp,
 24 lexer, 58 parser, 9 solve, 8 temporal)
$ rustup target add wasm32-unknown-unknown
(done locally; the target was not installed before this arc)
$ cargo check --target wasm32-unknown-unknown
(pass)
$ cargo check --target wasm32-unknown-unknown --all-targets
(pass)
```

New tests: 7 hdl tests (in `src/hdl.rs`, no simulator binary needed) —
the null backend reporting the no-simulator error from every operation
under its registry name with a no-op finish; its up-front load failure;
the registry answering the null backend the same way; unknown backend
names answering with the registered list (the `vhpi` spelling — the
documented future third backend, deliberately not registered);
`SignalSource::Real` dispatching to whatever backend is in the box
(verbatim error surfacing, the path the interpreter's `E0408` rides);
the default backend being one of the registered ones (xezim native /
null wasm, cfg-split); and the xezim backend keeping its registry name
and registration on native targets.

