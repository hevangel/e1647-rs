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
  <https://www.edaplayground.com/x/7Jrh>. Clicking Run opens
  `validatedAccessRequiredModal`: **"Your account is not validated"** —
  commercial simulators require a validated account. The confirmation email
  resend (`/resendConfirmation`) was requested. **Run output is therefore
  still not available**; the follow-up is owner-only: click the validation
  link in the mailbox of the `.env` account (check spam), then re-run the
  saved playground and record the log here. `'h`/`'x` hex and `0o` remain
  grammar-confirmed only until that run happens.
