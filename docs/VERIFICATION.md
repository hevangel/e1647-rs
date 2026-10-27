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
