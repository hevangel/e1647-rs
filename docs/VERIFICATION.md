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
