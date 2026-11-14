# e1647-rs — Migration from IEEE 1647-2016 to IEEE 1647-2019: Work Plan and Task List

**Repository:** `hevangel/e1647-rs` (snapshot `e1647-rs-main`, ~19.9 kLOC Rust: `src/lexer.rs`, `parser.rs`, `ast.rs`, `check.rs`, `elab.rs`, `interp.rs`, `solve.rs`, `temporal.rs`, `coverage.rs`, `macros.rs`, `builtins.rs`, `hdl.rs`)
**Source of truth for this plan:** the Techstreet redline *IEEE Std 1647-2019 (Revision of IEEE Std 1647-2016)*, 980 pages, blue = added, red = deleted. The implementing agents **do not have the PDF** — every task below therefore carries (a) the 2019 clause number, (b) a paraphrased specification written for this plan, (c) the exact syntax shape, and (d) independently authored test cases with expected results. Nothing in this document reproduces the standard's prose; agents must not paste standard text into the repository either (see `AGENTS.md`, "Lawful references only").

**Status of the repo today:** the code is written against the **2016** numbering and feature set (e.g. `src/lexer.rs` cites "Table 8 of the 2016 standard"; the docs cite 2016 clause numbers such as 4.1.2, 4.3.1.1, 4.5 Table 12, 5.8.4, 6.6, 8.1–8.3, 13.1.1, 27.4, 28.9). `docs/REFERENCES.md` already names 2019 as current, so the docs and code disagree.

---

## 0. How to use this document

1. Paste this file into a GitHub issue (or split each **Phase** into its own issue — the task IDs are stable).
2. Each task is tagged **[Opus]** or **[Sonnet]**:
   - **[Opus]** — design-heavy or semantics-heavy work (new type-system concepts, generation, dispatch, preprocessor expansion). Needs judgement about staging and about what to claim.
   - **[Sonnet]** — well-specified, mechanical work (lexer tables, parser productions for a fully specified grammar, builtin stubs, docs/clause renumbering, fixtures).
   - A task may say **[Opus design → Sonnet implement]**: Opus writes the AST/IR shape and the diagnostic policy in a short design note in the PR description, Sonnet implements against it.
3. Every task must obey the repository's rules (`AGENTS.md`, `CONTRIBUTING.md`):
   - Work on a feature branch; land through a reviewed PR; never push to `main`.
   - **Honest staged claims**: parsing ≠ checking ≠ elaboration ≠ execution. Update `README.md` status line, `docs/ROADMAP.md` checkboxes, `docs/ARCHITECTURE.md` and CLI help *in the same PR* that moves them.
   - Every new language behaviour ships with fixtures under `tests/fixtures/` (`.e` + `.expected`) and/or integration tests in `tests/*.rs`.
   - Run and report exactly: `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets`.
   - Record Specman reference runs (EDA Playground) in `docs/VERIFICATION.md` when a construct is qualified there; never copy vendor output.
4. **Test format reminders** (from `tests/fixtures.rs` and `tests/interp.rs`):
   - A fixture `tests/fixtures/<name>.e` is parsed with `e1647::parser::parse`; `<name>.expected` holds either the single line `OK` or one rendered diagnostic per line in the form `line:col: error[CODE]: message`. Columns are 1-based and point at the start of the offending token. Where this plan gives a diagnostic without a column, the implementer fills the column from the parser's actual span after confirming it lands on the named token.
   - Runtime tests use `run_source(source, seed)` / `clean_output(source, seed)` in `tests/interp.rs`; expected output is the exact text produced by `out()`. Use `out()` (not `print`) in new runtime tests so the expected text is format-stable.
5. **Diagnostic code allocation** for this migration (append to the module docs that list codes): parser syntax `E0121`–`E0139`; well-formedness (`check.rs`) `E0207`–`E0219`; elaboration (`elab.rs`) `E0304`–`E0319`; runtime (`interp.rs`) `E0411`–`E0429`. Codes are assigned per task below; if a code is already taken when you get there, take the next free one and update this table in the issue.
6. Suggested fixture naming: `tests/fixtures/s2019_<topic>.e` so the migration corpus is greppable.

---

## 1. Executive summary of what changed (2016 → 2019)

The 2019 revision is mostly **additive**. It introduces several genuinely new language features, generalises a few existing ones, adds a large block of predefined library (interfaces, tables, reflection, annotations), and **renumbers almost every clause** because each clause gained an "Introduction/Overview" subclause and because Clause 26 (Tables), Clause 31 (Predefined interfaces library) and Clause 35 (Annotations) were inserted.

### 1.1 New language features (affect lexer/parser/elab/interp)

| # | Feature | 2019 clause(s) | Impact on e1647-rs |
|---|---|---|---|
| F1 | **Interface types**: `interface I [like J,...] { method sigs }`, `struct S [like B] implementing I,... { }`, interface-typed variables/fields/params, dynamic dispatch, interface casting rules | 5.2.9, 5.4.4.4, 5.7.1, 5.10.2.4, 6.3, 6.13 | New declaration kind, new type kind, elaboration (conformance + conflicts), interpreter dispatch |
| F2 | **Static members**: `static` fields (singleton per type, `Type::field` access), `static` methods (no `me`), `static event`, `static on`, `on Type::event` | 4.4.3.1, 4.4.4.3, 4.17.4, 6.9.3, 11.1.4.1, 11.2, 17.2.2, 17.2.4 | Parser flags, elab storage model, interpreter static storage + restrictions |
| F3 | **Range-generated fields** `~f : t;`, the `rgf=` operator, constraints over rgf fields, routines `rgf_was_generated()`, `rgf_get_range()`, `rgf_was_reduced()` | 6.9.7, 10.3.13, 18.6, 29.13 | Field flag, solver produces a *set* instead of a value, new operator, new routines |
| F4 | **Field initial values** `f : t = const-exp;` (incl. `static const` mandatory init) | 6.9.1, 6.9.2, 6.9.4 | Parser + interpreter instance creation |
| F5 | **Templates rework**: value parameters `<tag'exp>: scalar-type`, `conditions { bool-exp; ... }` bounding, `implementing` in templates, template `interface` and template `numeric_type`, extended `template extend` | 8.2–8.8 | Parser + monomorphizer |
| F6 | **Custom numeric types**: `numeric_type T: S;` where `S implementing numeric_i of (S, signed, bits)`; predefined template interfaces `base_numeric_i`, `numeric_i`; `numeric_format_kind` | 5.2.15, 5.8.4, 8.4, Clause 31 | Depends on F1 + F5; interpreter operator dispatch |
| F7 | **64-bit scalars** `longint`, `longuint` (plus `nibble` now listed in Table 17) and the **six-way precision rules** (32-bit, 64-bit, unbounded, real) | 5.2.2 Table 17, 5.2.3.1, 5.6 | AST `ScalarBase`, interpreter arithmetic |
| F8 | New operators/expressions: `list_ptr==` / `!list_ptr=`; `(tuple) [not] in_table { rows }`; `evaluate [type] [(var)] { actions }`; `typeof(exp)` / `typeof_item(exp)` in type position; `::` static access; subtype qualifiers in dot access `p.red'good'x`; updated precedence Table 12 | 4.6 Table 12, 4.11.4, 4.11.8, 4.17.3, 4.17.4, 4.17.7, 5.9 | Lexer (multi-char operators), parser precedence, interpreter |
| F9 | **`chosen_from`** constraint: `keep target chosen_from read_only(list);` | 10.5.8 | Parser + solver/gen |
| F10 | **Tables** (new Clause 26): `table [count] table-data with { body }`, inline rows, header rows, `<#n>`/`<#name>`/`<#0>`/`<##...>` cell references, `table from operator(...) [using filter=, unique=]`, `csv_to_table()`, `excel_to_table()`; procedural API `table_manager`/`table`/`table_row` (32.3) | Clause 26, 32.3 | New preprocessor stage (like macros), new builtins |
| F11 | **Annotations** (new Clause 35): `[package] [repeatable] annotation @name { fields; methods }`, `@name(attr=val,...)` attached before declarations, annotation API | Clause 35, 32.1.2.13 | Parser + AST metadata + minimal API |
| F12 | Lexical/syntactic housekeeping: `/* */` multi-line comments are now standard (may span code segments; no nesting); a statement may span code segments; statements and struct members may be grouped in bare `{ }` blocks; name-resolution step for the literal name `global` | 4.2.3, 4.2.4, 4.3.2, 4.3.3, 4.4.5.3 | Lexer/parser |

### 1.2 Changed semantics / relaxed or tightened rules

| # | Change | 2019 clause | Notes |
|---|---|---|---|
| C1 | `extend global` is no longer an outright prohibition; extensions of `global` are "reserved for the implementation" | 4.4.2.2 | Decide whether `E0116` stays an error or becomes a warning (see T1.12) |
| C2 | Keyed-list key: must not be a `static` field; its type must not be a list or set (struct keys may contain static fields) | 5.2.11 | Check |
| C3 | `set_of_values()`/`full_set_of_values()` apply to any scalar type (word "numeric" dropped) | 5.10.5, 5.10.6 | Repo already handles enumerals; add bool |
| C4 | List pseudo-methods `max_value`, `min_value`, `average`, `product`, `sum` are *numeric* (not integer-only); result type = type of `exp` | 27.4.17, 27.4.20, 27.5.4–27.5.6 | Interp |
| C5 | `sort()` on a list of pointers: result undefined | 27.4.23 | Doc only |
| C6 | `for each in set` iterates any item type (enum/bool sets too) | 20.2.5 | Interp |
| C7 | `deep_copy()` generalised to scalars/strings/sets/lists | 29.2.2 | Builtin |
| C8 | Actions forbidden in static methods: `gen`, `check that`, `dut_error()`, `dut_errorf()`, `message`/SDM actions; `me` not usable | 10.6.2, 16.3.2.1, 16.3.3, 16.3.4, 24.4.1, 4.4.4.3 | Check (part of F2) |
| C9 | `set_check()` gains a `root_unit` parameter; new `set_check_by_name()` | 16.3.6, 16.3.7 | Builtins |
| C10 | Coverage API renames: `get_overall_grade()` split into `compute_overall_grade()` + `get_overall_grade()`; `get_ecov_name()` → `get_ucd_path()`; ".ecov file" wording → "coverage database" | 14.9.10–14.9.12 | Builtins/docs |
| C11 | Execution phases (Table 1): `extract`, `check`, `finalize` phases now call per-struct `extract()`, `check()`, `finalize()` (2016: `sys.xtract()`, `sys.check()`); `quit()` + `finalize()` in finalize phase; `rerun()` added | 1.5.5 Table 1, 28.3.3.6–28.3.3.8, 28.3.5.1 | Interp run loop |
| C12 | Generation sub-phases after `post_generate()`: `connect_pointers()` → `connect_ports()` → `check_generation()` on every unit, depth-first | 9.13.2–9.13.4 | Interp run loop |
| C13 | `on` struct member: static form and `on Type::event` form | 11.2 | Part of F2 |
| C14 | 64-bit integer precision rule changes the result of `longint`/`longuint` mixed comparisons vs `int(bits:64)`/`uint(bits:64)` | 5.6.3 | Part of F7 |

### 1.3 New predefined library surface (mostly stubs / honest "not supported" at this stage)

| Area | 2019 clause | Names |
|---|---|---|
| Unit attributes & methods | 7.3.4, 7.3.5, 7.4.4–7.4.7 | `pack_options()`, `per_inst_cover()` attributes; `get_children([with_ports])`, `get_ports()`, `get_ports_recursively()` |
| Port binding | 9.13.5–9.13.12 | `do_bind_unit()`, `connect()`, `remap_hdl_path()`, `disconnect()`, `disconnect_bound_set()`, `get_bound_set()`, `is_connected()` |
| Coverage | 14.8.2.10, 14.9.6, 14.9.15–14.9.17 | `covers.sample_cg()`, `set_per_inst_for_struct()`, `set_unicov_hier()`, `set_cross_instance_collection()`, `get_top_size_items()` |
| Messages | 24.9.2.14 | `user_message_handler()`, `user_transaction_handler()`, `user_transaction_start_handler()` hooks + `message_manager` helper tables 51–53 |
| Files | 30.5.16 | `files.flush_log_file()` |
| Range-gen routines | 29.13 | `rgf_was_generated()`, `rgf_get_range()`, `rgf_was_reduced()` |
| Predefined interfaces | Clause 31 | `base_numeric_i` (17 methods), `numeric_i` (52 methods), `numeric_format_kind` |
| Reflection | 32.1 (many), 32.2, 32.3 | `rf_object`, `rf_interface*`, `rf_range_generated_field`, `rf_on_method*`, `rf_annotation*`, template parameter types, `rf_custom_numeric`, `rf_manager.eval_exp_string()`, list element insert/delete, unit-hierarchy queries, `instance_iterator`, table API |

### 1.4 Clause renumbering 2016 → 2019 (for every comment, doc and diagnostic message in the repo)

The repo cites 2016 numbers throughout. Use this table for the sweep (T0.1). Entries marked † are inferred from the 2019 table of contents and should be sanity-checked against the TOC reproduced in §1.5 before editing.

| Topic cited in repo | 2016 number (as cited) | 2019 number |
|---|---|---|
| Code segments `<'` `'>` | 4.1.2 | 4.2.3 |
| Comments | 4.1.3 | 4.2.4 |
| Literals (unsized/sized/MVL/char) | 4.1.4.1 / 4.1.4.3 / 4.1.4.6 | 4.2.5.2 / 4.2.5.4 / 4.2.5.7 |
| Keywords table | Table 8 (4.1.6.4 in 2016) | Table 8, 4.2.6.4 |
| `extend global` restriction | 4.3.1.1 | 4.4.2.2 |
| Operator precedence Table 12 | 4.5 | 4.6 |
| String matching | 4.11 / 4.11.1 / 4.11.2 | 4.12 / 4.12.2 / 4.12.3 † |
| `is [not] a` | 4.16.1 † | 4.17.1 |
| Set type | 5.1.10 | 5.2.12 |
| `as_a()` | 5.8.1 | 5.10.2 |
| `set_of_values()` / `full_set_of_values()` | 5.8.4 / 5.8.5 | 5.10.5 / 5.10.6 |
| `struct` definition | 6.2 | 6.3 |
| `when` subtypes | 6.6 | 6.7 |
| Template declaration / template extend / instantiation | 8.1 / 8.2 / 8.3 | 8.2 / 8.6 / 8.7 |
| Event ports, `$` access | 9.4 / 9.4.1 | 9.5 (usage) & 9.7.3 (definition) / 9.5.2 & 9.7.7 |
| Connected field sets (CFS) | 10.2.7 | 10.3.6–10.3.7 † |
| `select` distributions | 10.2.12.2 | 10.3.12.3 |
| `on` / `on port$` / `expect` | 11.2 / 11.3 / 11.4 | 11.2 / 11.3 / 11.4 (unchanged) |
| Temporal operators 12.2.x | 12.2.8 … 12.2.16 | 12.2.9 … 12.2.17 (shifted by +1 because 12.2.1 "Introduction" was inserted) |
| Success/failure of TE | 12.3.1 / 12.3.2 | 12.3.2 / 12.3.3 |
| TCM scheduler / `sync` / `wait` | 13.1.1 / 13.1.2 / 13.1.3 | 13.2.2 / 13.2.3 / 13.2.4 † |
| Coverage: `cover` / `item` / `cross` / `transition` / extend | 14.1 / 14.2 / 14.3 / 14.4 / 14.5 | 14.2 / 14.3 / 14.4 / 14.5 / 14.6 |
| Per-instance coverage | 14.2.1 | 14.3.2 |
| Coverage configuration | 28.9 | 14.8 (Coverage API) / 14.9 (`covers` struct) / 28.7 † |
| Macros | 15.2 / 15.3 / 15.4 / 15.5.x / 15.6 | 15.2 / 15.3 / 15.4 / 15.5.x / 15.6 (15.4.x sub-numbers shift by +1) |
| `repeat … until` | 20.2.2 | 20.2.3 † |
| Preprocessor directives | 21.2 / 21.3 | 21.3 † (21.2 is now "Importing e modules") |
| List pseudo-methods | Clause 26 | Clause 27 |
| Set pseudo-methods | 27.4 | 28.5 |
| Predefined routines | Clause 28 | Clause 29 |
| Files | Clause 29 | Clause 30 |
| Reflection API | Clause 30 | 32.1 |
| Part 2 routine library | "Part 2" | Clauses 27–35 |

### 1.5 2019 top-level structure (for orientation)

1 Overview · 2 Normative references · 3 Definitions · 4 e basics · 5 Data types · 6 Structs, subtypes, and fields · 7 Units · 8 Template types · 9 e ports · 10 Generation constraints · 11 Events and temporal struct members · 12 Temporal expressions · 13 Time-consuming actions · 14 Coverage · 15 Macros · 16 Print and error handling · 17 Methods · 18 Creating and modifying e variables · 19 Packing and unpacking · 20 Control flow actions · 21 Importing modules · 22 Packages · 23 Simulation-related constructs · 24 Messages · 25 Sequences · **26 Tables (new)** · 27 List pseudo-methods · 28 Predefined methods · 29 Predefined routines · 30 Files · **31 Predefined interfaces library (new)** · 32 Predefined APIs and utilities (reflection 32.1, **instance_iterator 32.2 (new)**, **table API 32.3 (new)**) · 33 Semaphores · 34 Encryption · **35 Annotations (new)**.

---

## 2. Phase plan and dependency graph

```
Phase 0  Groundwork (docs, keywords, renumbering)             [Sonnet]   no deps
Phase 1  Lexical/syntactic foundation + simple operators       [Sonnet]   no deps
Phase 2  Static members                                        [Opus→Sonnet]  needs P1
Phase 3  Interfaces                                            [Opus]     needs P1
Phase 4  Templates rework                                      [Opus]     needs P3 (implementing), P1.4 (longint for value params)
Phase 5  Range-generated fields + rgf=                         [Opus]     needs P1
Phase 6  chosen_from                                           [Opus]     needs P1
Phase 7  Custom numeric types + 64-bit precision rules         [Opus]     needs P3, P4, P1.4
Phase 8  Execution phases & predefined library surface         [Sonnet]   needs P1 (parse), independent of P2–P7
Phase 9  Tables (Clause 26)                                    [Opus]     needs P1 (macro pre-pass knowledge)
Phase 10 Annotations (Clause 35)                               [Opus design→Sonnet]  needs P1
Phase 11 Reflection catalogue (32.1/32.2/32.3) as honest stubs [Sonnet]   after P2, P3, P5, P10 have AST shapes
Phase 12 Final docs/claims pass                                [Sonnet]   last
```

Phases 2, 3, 5, 6, 8, 9, 10 can run **in parallel** after Phase 1 lands, each on its own branch. Phase 4 and 7 wait for Phase 3. Phase 11 and 12 go last.

Sizing legend: **S** < ½ day of agent work, **M** ≈ 1 day, **L** ≈ 2–3 days, **XL** > 3 days (split into stages; claim only what lands).

---

## Phase 0 — Groundwork **[Sonnet]**

### T0.1 Clause-number sweep (S→M)
**Files:** `README.md`, `docs/ROADMAP.md`, `docs/ARCHITECTURE.md`, `docs/VERIFICATION.md` (only forward-looking text; do not rewrite historical log entries — add a note at the top of the log that entries before this date cite 2016 numbers), `src/*.rs` doc comments and diagnostic messages, `tests/*.rs` comments.
**Spec:** Replace every 2016 clause citation with the 2019 one from §1.4. Where a comment says "2016 standard" (e.g. `src/lexer.rs` line ~74, `tests/lexer.rs` line ~7, `docs/VERIFICATION.md` entry 20) say "IEEE 1647-2019" instead.
**Acceptance:** `grep -rn "2016" src tests docs README.md` returns only the historical VERIFICATION.md entries and the REFERENCES.md revision list. No diagnostic message text changes except clause numbers (so existing `.expected` files stay valid — run the fixture suite).

### T0.2 Keyword table alignment with 2019 Table 8 (S)
**Files:** `src/lexer.rs` (`KEYWORDS` + its doc comment), `tests/lexer.rs` (`REFERENCE_KEYWORDS` doc + the set-equality test).
**Spec:** The 2019 Table 8 single-word keyword set is (multi-word entries such as `all of`, `first of`, `is a`, `is also`, `is C routine`, `state machine`, `down to`, `C export`, `not in` are listed in the table as phrases and stay multi-token here):

```
abort and annotation as as_a assert assume attribute before bits break
buffer_port bytes case change check chosen_from compute computed conditions
context continue const cover cross cycle default define delay detach do each
edges else emit empty event exclusive_start exec expect export extend external
fail fall file final for force from gen if ifdef ifndef implementing in in_set
in_table index inout interface interface_port is it item keep keeping key like
line list matching me method_port method_type new none not now numeric_type of
on only or out package pass prefix prev print private protected range ranges
release repeat repeatable return reverse rise routine select set simple_port
soft start step stop struct suffix sync template that then tlm_initiator_socket
tlm_target_socket to transition true try type undefined unit until using var
when while with
```

Notes for the implementer:
- `static`, `evaluate`, `typeof`, `typeof_item`, `table`, `rgf`, `list_ptr` are **not** in Table 8 — they are contextual. Do not add them to `KEYWORDS`; the parser recognises them by position (see Phase 1/2).
- New in 2019 relative to the 2016 table (confirmed by the feature clauses): `annotation`, `chosen_from`, `conditions`, `implementing`, `in_table`, `interface`, `numeric_type`, `repeatable`. Add these to `KEYWORDS`. For the remaining Table 8 words that are currently missing from `KEYWORDS` (e.g. `abort`, `const`, `simple_port`, `method_port`, `export`, `external`, `final`, `inout`, `set`, `stop`, `pass`, `prefix`, `suffix`, `ranges`, `edges`, `none`, `context`, `computed`, `exclusive_start`, `in_set`, `interface_port`, `method_type`, `buffer_port`, `tlm_*_socket`, `attribute`, `ifdef`, `ifndef`, `out`): decide per word whether promoting it to a keyword breaks any existing fixture or corpus file (e.g. `out` is a routine name; `set` is a type name; `const` and `final` are already parsed contextually). Document each deliberate deviation in the `KEYWORDS` doc comment; keep the cross-check test's deviation list in sync.
- **Do not** make `static` a keyword: `static` is a legal field/method name in 2016 corpora? — check `local/` corpora if available; if none uses it, it is still safer contextual.
**Tests:** extend `tests/lexer.rs::keywords_match_reference_grammar_set` so the deviation list contains exactly the words listed in the doc comment; add `tokenizes_2019_keywords` asserting each of `annotation chosen_from conditions implementing in_table interface numeric_type repeatable` lexes as `TokenKind::Keyword`.

### T0.3 `docs/REFERENCES.md` and `README.md` designation (S)
Say explicitly that the implementation target is IEEE 1647-2019 and that behaviour notes cite 2019 clause numbers; keep the sentence that 2016 and earlier exist.

---

## Phase 1 — Lexical and syntactic foundation **[Sonnet]**

### T1.1 Multi-line comments are standard (4.2.4) (S)
**Current:** lexer accepts `/* */` but docs call it "an extension beyond clause 4.1.3".
**Spec (2019):** within code segments, `/* ... */` may start and end anywhere on a line, may span lines, and may span code segments. Comments do not nest: a second `/*` inside a comment is ignored, the first `*/` closes it. Single-line comments stay `--` and `//`.
**Work:** remove the "extension" wording from `src/lexer.rs` doc, `README.md`, `docs/ROADMAP.md`, `docs/ARCHITECTURE.md`. Add tests.
**Tests (`tests/lexer.rs`):**
```e
<'
struct foo {
    c: int; /* inline comment */
    d: int; /* starts here
               continues
            ends here */
    /* nested /* opener is ignored */ e: int;
};
'>
```
Assert the tokens between `{` and `}` are exactly `c : int ; d : int ; e : int ;` — i.e. `e: int;` **is** code because the inner `/*` did not open a new comment.
Fixture `s2019_block_comment_across_segments.e`:
```e
<'
struct a { /* comment starts in one segment
'>
prose between segments is skipped anyway
<'
   still a comment */ x: int; };
'>
```
Expected: `OK` (the comment closes in the second segment; `x: int;` parses).

### T1.2 Statement spanning code segments (4.2.3) (S)
**Spec:** a code segment may end in the middle of a statement; the statement must be completed in the next code segment of the same module.
**Fixture `s2019_statement_spans_segments.e`:**
```e
<'
struct pkt {
    a: uint;
'>
Documentation paragraph in the middle of the struct.
<'
    b: uint;
};
'>
```
Expected: `OK`. Add an `e1647 elab` integration assertion that `pkt` has fields `a`, `b`.

### T1.3 Brace-grouped statements and struct members (4.3.2, 4.3.3) (S→M)
**Spec:** one or more top-level statements may be grouped in `{ ... }`; one or more struct members may be grouped in `{ ... }`. Grouping is purely syntactic (no scope, no semantics). Blocks may nest.
**Work:** in `parse_declaration` (module level) and `parse_member_list` accept a bare `{` and splice the contained items into the enclosing list. Existing `#ifdef`-at-member-level behaviour must still work inside such blocks.
**Fixture `s2019_brace_groups.e`:**
```e
<'
{
    type color_t: [RED, GREEN];
    struct pkt {
        { a: uint; b: uint; }
        { keep a < 10; }
        c: color_t;
    };
}
extend sys {
    run() is also {
        var p: pkt;
        gen p;
        out("ok");
    };
};
'>
```
Expected: `OK`. Runtime test: output `ok`; `elab` lists `pkt` fields `a b c` in that order.

### T1.4 `longint`, `longuint`, `nibble` as predefined scalars (Table 17, 5.2.3.1) (M)
**Spec:** `longint` — signed 64-bit; `longuint` — unsigned 64-bit; default packing size 64; default value 0. `nibble` is in Table 17 (unsigned 4-bit) — already supported, just cite. **Subtypes may not be derived from `longint`/`longuint`** (`type t: longint [0..5]` and `type t: longint(bits:8)` are errors). `longint` is *not* the same as `int(bits:64)` — see T7.4 for the arithmetic rule.
**Work:** `ast.rs::ScalarBase` gains `Longint`, `Longuint`; `from_keyword`/`name`; `KEYWORDS` does not need them (reference grammar treats type names as identifiers; follow the existing `int`/`uint` handling). `check.rs`: new diagnostic **`E0207`** "subtype of 'longint'/'longuint' is not allowed (clause 5.2.3.1)". Interpreter: value ranges 64-bit on generation (`gen` of an unconstrained `longuint` must sample the full 64-bit range; i128 storage is fine).
**Fixture `s2019_longint_subtype.e`:**
```e
<'
type small_long: longint [0..7];
'>
```
Expected: `2:18: error[E0207]: subtype of 'longint' is not allowed` (column at `longint`).
**Runtime test:**
```e
<'
extend sys {
    run() is also {
        var a: longuint = 1;
        var b: longint = -1;
        out(a); out(b);
        var c: longuint = 18446744073709551615;
        out(c);
    };
};
'>
```
Expected output lines: `1`, `-1`, `18446744073709551615`.

### T1.5 Operator precedence Table 12 (4.6) (S)
**Spec (2019 Table 12, tightest first):** `[]` index · `[..]` slice · `[:]` bit slice · `f()` call · **`::` static access** · `.` dot · `~ ! not` unary · `{...;...}` list concat · `%{...}` bit concat · unary `+ -` · `* / %` · binary `+ -` · `>> <<` · `< <= > >=` · `is a / is not a` · `== != list_ptr== !list_ptr=` · `=== !==` · `~ !~` string match · `in not in` (and `in_table`, see T1.7) · `&` · `|` · `^` · `&& and` · `|| or` · `=>` · `? :`.
**Work:** update `BINARY_LEVELS`/`binary_row_for` doc in `parser.rs`; add `list_ptr==`/`!list_ptr=` to the equality row (T1.6); `::` is a postfix/primary-level construct (T2.1).
**Test:** `tests/parser.rs` precedence test: `a == b list_ptr== c` parses left-assoc at one level; `x :: y . z` parses as `(x::y).z`.

### T1.6 `list_ptr==` and `!list_ptr=` (4.11.4) (S)
**Spec:** both operands must be lists of exactly the same type. `l1 list_ptr== l2` is TRUE iff both expressions denote the *same list object* (pointer identity), FALSE for distinct lists even with equal contents. `!list_ptr=` is the negation. Regular `==` stays item-by-item.
**Work:** lexer: longest-match tokens `list_ptr==` and `!list_ptr=` (note `list_ptr` alone is an identifier). AST `BinaryOp::ListPtrEq`, `ListPtrNe`. Interpreter: lists need an identity — if `Value::List` is a `Vec` by value today, introduce an `Rc<RefCell<..>>` or a list id so that assignment `var l3 = l1` aliases while `l1.copy()` allocates. Runtime diagnostic **`E0411`** "list_ptr== operands must be lists of the same type".
**Runtime test:**
```e
<'
extend sys {
    run() is also {
        var l1: list of int = {1; 2; 3};
        var l2: list of int = l1.copy();
        var l3: list of int = l1;
        out(l1 list_ptr== l2);
        out(l1 list_ptr== l3);
        out(l1 !list_ptr= l2);
        out(l1 !list_ptr= l3);
        out(l1 == l2);
    };
};
'>
```
Expected: `FALSE`, `TRUE`, `TRUE`, `FALSE`, `TRUE`.
Fixture `s2019_list_ptr_type_mismatch.e` with `var a: list of int; var b: list of uint; out(a list_ptr== b);` → runtime `E0411` (write as an interp test asserting the diagnostic code).

### T1.7 `in_table` (4.11.8) (M)
**Spec:** `(exp1, exp2, ..., expN) [not] in_table { row; row; ... }` where each row is `elem, elem, ..., elem` with exactly N elements. Result TRUE iff some row matches the tuple: a scalar row element matches by `==`; a *set* row element (any set expression for numeric tuple elements; a constant set for enum/bool elements, written as a set literal, `set_of_values()` or `full_set_of_values()`) matches by `in`. `not` negates. In a `keep`, the construct constrains the tuple to the rows (it is a disjunction of conjunctions).
**Work:** parser: in the `in` precedence row accept `in_table` after a parenthesised tuple (a tuple is otherwise not an expression — parse `( e, e, ... )` as `Expr::Tuple` only when followed by `[not] in_table`). Rows are `;`-separated inside `{ }`. Interpreter: evaluate as described. Solver/`gen`: translate `keep (a,b) in_table {...}` to rejection sampling (the IR has no disjunction yet — document this honestly) and make sure the sampler's domain narrowing does not mis-narrow.
Diagnostics: **`E0121`** "in_table row has N elements, tuple has M" (parser, when counts differ); **`E0412`** runtime type mismatch between tuple element and row element.
**Runtime test:**
```e
<'
type speed_t: [LOW, HIGH, FULL];
type kind_t: [DATA, INTERRUPT, CONTROL];
struct pipe {
    kind: kind_t;
    speed: speed_t;
    size: uint;
    keep (kind, speed, size) in_table {
        INTERRUPT, FULL, [0..300];
        CONTROL,   HIGH, 100;
        DATA,      LOW,  1;
    };
};
extend sys {
    run() is also {
        var p: pipe;
        for i from 1 to 20 {
            gen p;
            if (p.kind, p.speed, p.size) not in_table {
                INTERRUPT, FULL, [0..300];
                CONTROL,   HIGH, 100;
                DATA,      LOW,  1;
            } then { out("BAD") };
        };
        out((DATA, LOW, 1) in_table { DATA, LOW, 1 });
        out((DATA, LOW, 2) in_table { DATA, LOW, 1 });
        out((DATA, LOW, 2) not in_table { DATA, LOW, 1 });
    };
};
'>
```
Expected: no `BAD` lines; then `TRUE`, `FALSE`, `TRUE`.
**Fixture `s2019_in_table_arity.e`:** `keep (a, b) in_table { 1, 2, 3; };` → `E0121` at the row's first element.

### T1.8 `evaluate { ... }` expression (4.17.7) (M)
**Spec:** `evaluate [type] [(var-name)] { action; ... }` is an *expression*. The actions run; the value of the implicit variable (`value` by default, or `var-name` if given) after the block is the expression's value. The variable's type is `type` if given, else inferred from context; if it cannot be inferred → compile error. The variable is visible only inside the block and starts at its type's default value. Actions may not include temporal expressions (`wait`/`sync`) nor `return`.
**Work:** `Expr::Evaluate { ty: Option<TypeExpr>, var: Option<String>, body: Vec<Stmt> }`. Type inference v1: the declared type of the assignment/`var` target, parameter type, or the `type` argument; otherwise **`E0122`** "evaluate: result type cannot be inferred; write `evaluate <type> { ... }`". `return`/`wait`/`sync` inside → **`E0123`**.
**Runtime test:**
```e
<'
extend sys {
    run() is also {
        var x: int = evaluate { value = 3; value = value * 4 };
        var y: uint = evaluate uint (acc) { acc = 1; for i from 1 to 4 { acc = acc * 2 } };
        out(x); out(y);
    };
};
'>
```
Expected: `12`, `16`.
Fixture `s2019_evaluate_return.e`: `evaluate int { return 1; }` → `E0123`.

### T1.9 `typeof()` and `typeof_item()` in type position (5.9) (M)
**Spec:** wherever a type name is expected, `typeof(exp)` denotes the static type of `exp` (the expression is never evaluated), and `typeof_item(exp)` denotes the element type of a list-typed `exp`. Purely compile-time. Typical use is inside macros.
**Work:** `TypeExpr::TypeOf(Box<Expr>)`, `TypeExpr::TypeOfItem(Box<Expr>)`. Resolution happens in `elab.rs`/`check.rs` against the (currently minimal) type information: v1 resolves `typeof` of a field path, local `var`, method parameter, or literal; anything else → **`E0208`** "typeof(): cannot determine the static type of this expression". `typeof_item` of a non-list → **`E0209`**.
**Runtime test:**
```e
<'
struct s {
    data: list of byte;
    first_item: typeof_item(data);
    n: typeof(first_item);
};
extend sys {
    run() is also {
        var x: s = new;
        x.first_item = 200;
        x.n = x.first_item + 55;
        out(x.n);
        var k: typeof(x.n) = 7;
        out(k);
    };
};
'>
```
Expected: `255`, `7` (and `x.n` must be a `byte` — add an `elab` assertion that field `n` resolves to `byte`).
Fixture `s2019_typeof_item_nonlist.e`: `b: typeof_item(a);` where `a: int` → `E0209`.

### T1.10 Subtype qualifiers in dot access (4.17.3) (M)
**Spec:** `struct-exp.qual'qual'...'field` accesses a field declared in a `when` subtype. A qualifier sequence is legal iff the same words separated by spaces would be a legal when-subtype name; qualifiers already implied by the static type of `struct-exp` may be omitted. Each qualifier can be short (`red`) or full (`red'color`).
**Work:** parser: after `.`, allow `name '` sequences: `red'good'x`, `TRUE'good'red'x`, `red'color'x`. AST `Expr::QualifiedField { base, qualifiers: Vec<WhenQualifier>, field }`. Elab/interp: resolve against the synthesized when views (`src/elab.rs` already builds `LO HI pkt` views) — the field is found in the view whose when-of set contains all qualifiers. Runtime: if the instance does not currently match the subtype → **`E0413`** "field 'x' is not present: instance is not of subtype 'red good packet'".
**Runtime test:**
```e
<'
type color_t: [red, green, blue];
type size_t: [big, medium, small];
struct packet {
    color: color_t;
    size: size_t;
    good: bool;
    when red good packet { x: int; keep x == 5; };
    when big'size green'color packet { y: uint; keep y == 9; };
};
extend sys {
    p1: good packet;  keep p1.color == red;
    p2: packet;       keep p2.color == green; keep p2.size == big;
    run() is also {
        out(p1.red'good'x);
        out(p1.TRUE'good'red'x);
        out(p1.red'color'x);
        out(p2.green'color'big'size'y);
        out(p2.big'green'y);
    };
};
'>
```
Expected: `5`, `5`, `5`, `9`, `9`.

### T1.11 Field initial values (6.9.1, 6.9.2, 6.9.4) (M)
**Spec:** field syntax is now
`[package|protected|private] [static] [const] [!|~] [%] name[: type] [[lo..hi]] [((bits|bytes): n)] [= init-value]`.
Rules: an init value is allowed only for a scalar, string, set, or a list thereof (not for `const` list fields); it must be a constant expression; **not allowed on generated fields** (so in practice it requires `!`, or `static`, or `const`); a `static const` field **must** have an init value; a `const` field may be initialised (a) by its init value, (b) in `init()`, (c) by `do_unpack()`, and only once. `~` and `!` are mutually exclusive; `~` may combine with `%`; `~` fields may not be `const`.
**Work:** `Field` gains `is_static: bool`, `is_const: bool`, `range_gen: bool`, `init: Option<Expr>`. Parser accepts the full prefix order (any order of `!`/`~` and `%` as today; `static`/`const` before them). `check.rs` diagnostics: **`E0210`** is taken — use **`E0211`** "initial value is not allowed on a generated field (declare it `!`)", **`E0212`** "static const field requires an initial value", **`E0213`** "`!` and `~` cannot be combined", **`E0214`** "range-generated field cannot be const". Interpreter: on instance creation set initialised fields before `init()`.
**Runtime test:**
```e
<'
struct cfg {
    !depth: uint = 16;
    !name: string = "dut";
    !mask: list of bit = {1; 0; 1};
    const !limit: uint = 3;
};
extend sys {
    run() is also {
        var c: cfg = new;
        out(c.depth); out(c.name); out(c.mask.size()); out(c.limit);
    };
};
'>
```
Expected: `16`, `dut`, `3`, `3`.
Fixtures: `s2019_init_on_generated.e` (`depth: uint = 16;`) → `E0211`; `s2019_bang_tilde.e` (`!~x: uint;`) → `E0213`.

### T1.12 `extend global` policy (4.4.2.2) (S) — decision task
**Spec change:** 2016 said the global struct shall not be extended; 2019 says extensions of `global` are intended to be reserved for the implementation. Additionally (new): static fields and static methods **cannot** be added to `global` (6.9.3, 17.2.2).
**Decision for the implementer (Opus may be consulted):** keep `E0116` as an error for user code (conservative, matches "reserved for the implementation") but reword the message to "extending 'global' is reserved for the implementation (clause 4.4.2.2)". Update `tests/fixtures/extend_global.expected` accordingly.

### T1.13 Name resolution: literal `global` (4.4.5.3) (S)
**Spec:** for a path-less name, after the existing checks, the resolver first checks whether the name is literally `global` (which denotes the global struct) and only then whether it names a member of `global`. Implement in `interp.rs` name lookup; test that `out(global.sys != NULL)`-style access is not broken (if `global` is modelled). If `global` is not modelled yet, document and skip with a note.

---

## Phase 2 — Static members **[Opus design → Sonnet implement]**

Design note required from Opus before implementation: where static storage lives (`Interp` per-type map keyed by canonical type name), how `when` subtypes are excluded, how extension (`extend T { static ... }`) is handled, and how `Type::field` and `Type::event` resolve through `elab.rs` composed types.

### T2.1 Static fields and `::` access (6.9.3, 4.17.4, 4.4.3.1) (L)
**Spec:** a `static` field belongs to the type; one shared occurrence for all instances; exists even with zero instances; created at declaration; initialised automatically (init value or type default). Restrictions: not in `when` subtypes; not generated; not physical (`%`); not a unit instance nor a list of unit instances (7.2.2, 7.2.4); cannot be added to `global`. Access: `Type::field` or, inside the type (or its children), bare `field`. A path may *start* at a static field and continue through non-static fields. The static field must be declared in exactly `Type` (not a parent) for `Type::field` to be legal. The dot operator refers only to non-static members.
**Work:** parser: `static` prefix on fields (contextual word followed by `const`/`!`/`%`/`~`/name). `Expr::StaticAccess { type_name, field }` for `T::f` (with the `[type::]` prefix optional → plain name resolution). Elab: collect static fields per type; diagnostics **`E0304`** "static field in when subtype", **`E0305`** "static field cannot be physical or generated" (also for `~`), **`E0306`** "static field of unit-instance type", **`E0307`** "'T::f': 'f' is not a static field declared in 'T'", **`E0308`** "static member cannot be added to 'global'". Interp: static store initialised at program start; `gen` skips static fields; `print`/`to_string` of an instance do not include static fields.
**Runtime test:**
```e
<'
struct counter {
    static count: uint;
    static !label: string = "ctr";
    id: uint;
    init() is also { counter::count = counter::count + 1; };
};
extend sys {
    run() is also {
        out(counter::count);
        var a: counter = new; var b: counter = new; var c: counter = new;
        out(counter::count);
        out(counter::label);
        out(a.id == b.id or TRUE);
    };
};
'>
```
Expected: `0`, `3`, `ctr`, `TRUE`.
Fixtures: `s2019_static_in_when.e` → `E0304`; `s2019_static_physical.e` (`static %x: uint;`) → `E0305`; `s2019_static_wrong_type.e` (`struct a { static s: uint; }; struct b like a { }; ... b::s`) → `E0307`.

### T2.2 Static methods (17.2.2, 17.2.4) (M)
**Spec:** `static name(params) [: ret] is { ... }`. Callable as `Type::name(...)` without an instance. Inside: `me` is illegal; may read/write static fields of the same type; may not contain `gen`, `check that`, `dut_error()`, `dut_errorf()`, `message`/`messagef`/SDM actions. A static method can be extended (`is also/first/only`) only in the exact same struct, not in a child; not in `when` subtypes; not added to `global`. Not a TCM.
**Work:** `MethodDecl.is_static`. `Expr::StaticCall`. `check.rs`: **`E0215`** "'me' cannot be used in a static method", **`E0216`** "'gen'/'check that'/'dut_error'/'message' is not allowed in a static method", **`E0217`** "static method 'f' can only be extended in the struct that declares it" (elab may be the better home — `E0309`). Interp: dispatch without receiver.
**Runtime test:**
```e
<'
struct math {
    static sum(a: int, b: int): int is { return a + b; };
    static twice(a: int): int is { return math::sum(a, a); };
};
extend sys {
    run() is also { out(math::sum(2, 3)); out(math::twice(21)); };
};
'>
```
Expected: `5`, `42`.
Fixtures: `s2019_static_me.e` (`static f() is { out(me); }`) → `E0215`; `s2019_static_gen.e` → `E0216`; `s2019_static_extend_child.e` → `E0309`.

### T2.3 Static events, `static on`, `on Type::event` (11.1.4.1, 11.2) (M)
**Spec:** `static event name;` declares an event of the type itself (no instance; not in `when`; cannot be extended; not on `global`). `static on event-type { actions }` runs on a static event of the context struct (declared directly in this struct, not a parent); the implicit `on_<event>()` method is then static. `on Type::event { actions }` is the explicit form. `emit Type::event;` triggers it.
**Work:** `EventDecl.is_static`, `OnDecl.is_static`, `OnDecl.type_qualified: Option<String>`. Elab: static `on` blocks compose per type, not per instance. Interp: `emit T::e` runs the static `on` blocks with no `me` (same restriction as T2.2); the temporal log records the occurrence like any other event.
**Runtime test:**
```e
<'
struct bus {
    static event reset;
    static on reset { out("bus reset seen"); };
};
extend sys {
    run() is also { emit bus::reset; emit bus::reset; };
};
'>
```
Expected: `bus reset seen` twice.
Fixture `s2019_static_event_when.e` (static event inside a when block) → `E0304` (reuse) or a dedicated `E0310` "static event in when subtype".

### T2.4 Keyed-list key restrictions (5.2.11) (S)
**Spec:** the key may not be a static field and its type may not be a list or set; a struct-typed key may contain static fields.
**Work:** `check.rs` **`E0218`** "keyed list key cannot be a static field or of list/set type". Fixture `s2019_keyed_list_static_key.e`.

---

## Phase 3 — Interfaces **[Opus]**

### T3.1 `interface` declaration (6.13) (M)
**Syntax:** `[package] interface name [like base1, base2, ...] { member; ... }` where each member is a method *signature* `name([params]) [: ret-type];` (no body — like an undefined method). An interface may inherit from several interfaces and includes all their members. Interfaces may be `extend`ed? — 2019 text only shows declaration; treat `extend I {...}` as **`E0124`** unless the implementer finds otherwise.
**Work:** `DeclarationKind::Interface`, `Declaration.bases: Vec<String>` for interfaces. `check.rs`: **`E0125`** "interface member must be a method signature" (fields/constraints/events inside an interface), **`E0219`** "interface 'I' inherits conflicting signatures for 'm'".
**Fixture `s2019_interface_decl.e`:**
```e
<'
interface printable { to_text(): string; };
interface sized { size_of(): uint; };
interface both like printable, sized { describe(prefix: string): string; };
'>
```
Expected: `OK`; `elab` lists `both` with methods `to_text size_of describe`.
Fixture `s2019_interface_field.e` (`interface i { x: int; };`) → `E0125`.

### T3.2 `implementing` on structs and extends (6.3, 31.4) (M)
**Syntax:** `struct S [like B] implementing I1, I2, ... { ... }`, `unit U ... implementing ... { }`, and `extend S implementing I { ... }` (adds interfaces to an existing type; the 2019 numeric-type clause shows this form).
**Semantics:** S is expected to define every method of every implemented interface with the same signature; a conflict is two method declarations with the same name and different signatures (between interfaces, or between an interface and the base struct) → compile error. A struct may implement any number of interfaces in addition to one `like` base. Subtypes (`like`/`when`) of S implement S's interfaces.
**Work:** `Declaration.implements: Vec<String>`. Elab: record `implements` on composed types, check conflicts (**`E0311`** "'S' implements 'I' and 'J' which declare 'm' with different signatures"), **do not** error on a missing implementation at elaboration (the standard makes it a *runtime* error when called — T3.4), but emit nothing.
**Fixture `s2019_implementing_conflict.e`:**
```e
<'
interface a { f(x: int); };
interface b { f(x: string); };
struct s implementing a, b { };
'>
```
Expected: `E0311` at `s`.

### T3.3 Interface types in the type system (5.2.9, 5.4.4.4, 5.7.1, 5.10.2.4) (L)
**Spec:** an interface name is a type usable for variables, fields, parameters, return values, list elements. No object is *of* an interface type; a value of interface type is a pointer to a struct that implements it; default value `NULL`. Assignment of a struct to an interface-typed target is an automatic cast when the struct's type implements the interface (directly or via inheritance); interface → base interface is automatic too. `as_a()`: between related struct types as before; struct ↔ interface it implements; between **two unrelated interface types is legal** (a struct may implement both) and yields `NULL` on mismatch; between two unrelated struct types → compile error (existing behaviour); interface ↔ unrelated struct type → legal (a subtype might implement it later).
**Work:** `TypeExpr::Named` already exists; elab must classify names as struct / interface. `check.rs`/`elab.rs`: **`E0312`** "cannot assign 'S' to interface 'I': 'S' does not implement 'I'". Interp: `is a <interface>` must be TRUE for implementors; `as_a(I)` returns the same pointer or NULL.
**Runtime test:**
```e
<'
interface shape { area(): uint; };
struct square implementing shape { side: uint; area(): uint is { return side * side; }; };
struct rect   implementing shape { w: uint; h: uint; area(): uint is { return w * h; }; };
struct blob { };
extend sys {
    run() is also {
        var sq: square = new; sq.side = 3;
        var rc: rect = new; rc.w = 2; rc.h = 5;
        var shapes: list of shape = {sq; rc};
        var total: uint = 0;
        for each (s) in shapes { total = total + s.area(); };
        out(total);
        var sh: shape = sq;
        out(sh is a square);
        out(sh.as_a(rect) == NULL);
        var b: blob = new;
        out(b is a shape);
    };
};
'>
```
Expected: `19`, `TRUE`, `TRUE`, `FALSE`.
Fixture `s2019_interface_assign_mismatch.e` (`var s: shape = new blob;`) → `E0312`.

### T3.4 Dynamic dispatch and missing-implementation error (6.13) (M)
**Spec:** calling an interface method through an interface-typed expression invokes the method of the object's actual struct type at run time. If that struct type has not implemented the method → runtime error, like calling an undefined method.
**Work:** interp method lookup through interface; runtime **`E0414`** "struct 'S' does not implement interface method 'm'". Test: `struct lazy implementing shape { }` then `var s: shape = new lazy; s.area();` → `E0414`.

### T3.5 `print`/`to_string` of interface-typed values (S)
Print the underlying struct as today; `out(sh)` of NULL prints `NULL`. Document.

---

## Phase 4 — Templates rework **[Opus]**

The 2016 template model (type parameters only, categories, defaults) is replaced by a richer one. Keep the monomorphizer in `elab.rs` but widen its parameter model.

### T4.1 Value parameters (8.5.2, 8.2.1) (M)
**Syntax:** parameter list entries are either type parameters `<[tag']type> [: type-category] [= default-type]` or value parameters `<[tag']exp> : scalar-type [= default-value]`. If there is exactly one type parameter with no category and no default, the parentheses around the list may be omitted (existing `template struct a of <t'type>` form).
**Semantics:** a value parameter has a scalar type (numeric, enum, bool; not a custom numeric type) and is substituted by a constant where a constant expression of that type is expected (field widths, ranges, constraints, method bodies). Actuals are constant expressions. Instance canonical name renders the value (`buf of (byte, 8)`).
**Work:** `TemplateParam` gains `kind: Type | Value { scalar: TypeExpr }`, `default: Option<TemplateDefault::{Type, Value(Expr)}>`. Instantiation substitutes `<n'exp>` occurrences in `Expr` trees. Diagnostics: **`E0126`** "template value parameter requires a scalar type", **`E0313`** "actual for value parameter 'n' is not a constant expression of type 'uint'".
**Runtime test:**
```e
<'
template struct fifo of (<t'type>, <depth'exp>: uint = 4) {
    !items: list of <t'type>;
    capacity(): uint is { return <depth'exp>; };
    push(x: <t'type>): bool is {
        if items.size() >= <depth'exp> then { return FALSE };
        items.add(x); return TRUE;
    };
};
extend sys {
    run() is also {
        var f8: fifo of (byte, 8) = new;
        var f4: fifo of (int) = new;
        out(f8.capacity()); out(f4.capacity());
        for i from 1 to 5 { compute f4.push(i) };
        out(f4.items.size());
    };
};
'>
```
Expected: `8`, `4`, `4`.

### T4.2 Bounding: `conditions { ... }` and type categories (8.5.3) (M)
**Syntax:** after the base/implementing clauses and before the body: `conditions { bool-exp; bool-exp; ... }`. Each `bool-exp` must depend on at least one parameter and is a constant expression over value parameters (and type categories for type parameters remain `: scalar`, `: numeric`, `: struct`, … — keep the 2016 category list already parsed; validate at instantiation).
**Semantics:** an instance whose actuals violate any condition or category is invalid → compile error at the instantiation site. In a `template extend` (T4.4) failing conditions mean the extension simply does not apply to that instance.
**Work:** `Declaration.template_conditions: Vec<Expr>`. Instantiation evaluates conditions with the constant evaluator used for `select` choices; **`E0314`** "template instance 'map of (int, 150)' violates condition '<max_size'exp> % 100 == 0'"; **`E0315`** "actual type 'packet' for parameter <key'type> is not in category 'scalar'".
**Fixture `s2019_template_condition.e`:**
```e
<'
template struct map of (<key'type>: scalar, <value'type> = int, <max_size'exp>: uint = 1000)
    conditions { <max_size'exp> > 0; <max_size'exp> % 100 == 0 } {
    !n: uint;
};
extend sys {
    good: map of (uint, string, 300);
    bad:  map of (uint, string, 150);
};
'>
```
Expected: `E0314` at the `bad` field's type (and elab passes for `good`).

### T4.3 `implementing` in template structs (8.2.3) (S after T3.2)
**Syntax:** `template struct name of (...) [like base] [implementing I1, I2 of <t'type>, ...] [conditions {...}] { ... }`. Implemented interfaces may be concrete or parameterised over the template parameters; substitution happens on instantiation (8.7). Add to the monomorphizer; test with `template struct box of <t'type> implementing printable {...}` and an `is a printable` check on an instance.

### T4.4 `template extend` (8.6) (M)
**Syntax:** `template extend name of (param-list) [implementing I,...] [conditions { ... }] { member; ... }` where `param-list` entries are: type params `<tag'type> [: category]` **or** a concrete `type-name`; value params `<tag'exp>: scalar-type` **or** a `constant-value`. Parameter count must equal the declaration's; tags may differ from the declaration (and the extension body must then use its own tags); parentheses may be omitted for a single unqualified type parameter. At least one entry must be a parameter form (all-concrete lists are an error).
**Semantics:** the extension applies to every instance — existing or future — whose actuals match the concrete entries, satisfy the categories and the `conditions`. 2016's "applies if type parameters comply" is generalised to "bounding criteria".
**Work:** today the repo records `template extend` as unsupported (8.2). Implement: `Declaration.template_extend: Option<TemplateExtendSpec>`; at instantiation, for each instance apply matching extensions (members appended in declaration order). **`E0127`** "template extend: parameter count mismatch", **`E0128`** "template extend: at least one parameter must be a <tag'type>/<tag'exp> parameter".
**Runtime test:**
```e
<'
type color_t: [red, green];
template struct packet of (<k'type>, <w'exp>: uint) { !kind: <k'type>; };
template extend packet of (<e'type>: enum, <n'exp>: uint) conditions { <n'exp> >= 8 } {
    wide(): bool is { return TRUE; };
};
template extend packet of (color_t, 4) {
    narrow(): bool is { return TRUE; };
};
extend sys {
    run() is also {
        var a: packet of (color_t, 8) = new;
        var b: packet of (color_t, 4) = new;
        out(a.wide()); out(b.narrow());
    };
};
'>
```
Expected: `TRUE`, `TRUE`; additionally an elab assertion that `packet of (color_t, 4)` has **no** `wide` method and `packet of (color_t, 8)` has **no** `narrow` method.

### T4.5 Defaults derived from earlier parameters, including value → type (8.5.4) (S)
**Spec example shape:** `template struct t1 of (<first'type>, <first'exp>: int, <second'exp>: int = <first'exp> + 5, <second'type> = <first'type>(bits: <first'exp>), <third'type> = list of <first'type>) {}`. Defaults must be at the end of the list; omitted trailing actuals take defaults; you cannot skip a parameter.
**Test:** `t1 of (uint, 4)` renders canonical name `t1 of (uint, 4, 9, uint(bits: 4), list of uint)` — assert via `elab` output; **`E0129`** "a parameter with a default cannot be followed by one without".

### T4.6 Template interfaces and template numeric types (8.3, 8.4) (M, after T3.1 / T7.1)
**Syntax:** `template interface name of (params) [like I, J of <t'type>] { sigs }`; `template numeric_type name of (params): impl-struct of (...);`. Instances become interfaces / numeric types when referenced. Parameters substitute in bases / implementing struct. Fixtures: parse-clean + `elab` listing; runtime via T3.x/T7.x tests.

### T4.7 Instantiation substitution into bases and interfaces (8.7) (S)
Already partly done for bases; extend to `implementing` lists and interface bases; canonical rendering of value actuals; same actuals share one instance. Covered by tests above.

---

## Phase 5 — Range-generated fields **[Opus]**

### T5.1 `~` field modifier (6.9.7, 6.9.1) (S)
Parsed in T1.11 (`Field.range_gen`). Here: elab/check rules — `~` not with `!`, not `const`, not `static`; allowed with `%`. Only scalar fields (numeric, enum, bool) may be range-generated → **`E0316`** "range-generated field must be scalar".

### T5.2 Generation of a reduced *set* (10.3.13) (L)
**Spec:** generating a struct with `~` fields produces, for each such field, a *reduced set* of values such that **any** value from the set satisfies the constraints given the concrete values of the other (non-rgf) fields; the field also receives an implementation-chosen concrete value from that set. Only constraints of the shape `keep [soft] [bool-exp bool-op] rgf-exp rel-op exp [bool-op bool-exp]` are supported, where `bool-op ∈ {&&, ||, =>}`, `rel-op ∈ {==, !=, in, not in, >, >=, <, <=}`, `bool-exp`/`exp` contain no rgf fields, `exp` is not a `select` and has no generative bitwise operators. Other constraint shapes involving an rgf field → **`E0317`** "unsupported constraint shape for range-generated field 'a'".
**Algorithm v1 (document it):** generate all non-rgf fields first (existing sampler/solver); then for each rgf field compute the set as the intersection over applicable constraints of the interval(s) implied by `rel-op exp` (with `exp` evaluated to a constant) — an implication whose antecedent is FALSE contributes the full type range; `||` forms contribute the union; the concrete value is the set's minimum (deterministic; state this in the docs, it is implementation-defined per the standard).
**Runtime test (exactly the worked example shape):**
```e
<'
struct rg {
    c: bool;
    ~b: uint;
    ~a: uint;
    keep c => a < 10;
    keep !c => b < 10;
};
extend sys {
    run() is also {
        var r: rg;
        for i from 1 to 10 {
            gen r;
            if r.c then {
                out(rgf_get_range(r.a) == [0..9]);
                out(rgf_get_range(r.b) == [0..MAX_UINT]);
            } else {
                out(rgf_get_range(r.b) == [0..9]);
                out(rgf_get_range(r.a) == [0..MAX_UINT]);
            };
            out(r.a in rgf_get_range(r.a) and r.b in rgf_get_range(r.b));
        };
    };
};
'>
```
Expected: 30 lines, all `TRUE`.

### T5.3 `rgf=` operator (18.6) (S)
**Spec:** `field rgf= exp;` assigns a concrete value to an already-generated rgf field; the value must belong to the generated set, else runtime error; no automatic width cast — a value that does not fit is also an error. Not usable on non-rgf fields (compile error).
**Work:** lexer token `rgf=`; `Stmt::RgfAssign`. **`E0130`** parser/check "rgf= target is not a range-generated field"; runtime **`E0415`** "value 12 is outside the generated range [0..9] of 'a'"; **`E0416`** "rgf= on a field that was not generated".
**Runtime test:** after `gen r` with `c == TRUE` forced by `keep c == TRUE`: `r.a rgf= 7; out(r.a);` → `7`; `r.a rgf= 12;` → `E0415`.

### T5.4 Routines `rgf_was_generated()`, `rgf_get_range()`, `rgf_was_reduced()` (29.13) (S)
**Spec:** `rgf_was_generated(f)` TRUE iff the solver generated `f` (compile error for a non-rgf field); `rgf_get_range(f)` returns the generated set (error if not generated yet) — usable where a numeric set is expected, and for enum/bool fields where a values set of that type is expected; `rgf_was_reduced(f)` TRUE iff the set is smaller than the declared type's full set (error if not generated).
**Work:** builtins in `builtins.rs`/`interp.rs`, backed by per-instance rgf metadata. Tests folded into T5.2/T5.3 plus: before any `gen`, `rgf_get_range(r.a)` → **`E0416`**; `rgf_was_reduced(r.a)` after `gen` with `c == TRUE` → `TRUE`, and for `r.b` → `FALSE`.

---

## Phase 6 — `chosen_from` **[Opus]**

### T6.1 `keep target chosen_from read_only(list)` (10.5.8) (L)
**Spec:** the target (a simple generative struct/unit path: not an input, no `!` field on the path, no list index, not a complex expression) must be assigned a pointer to one of the elements of the input list, chosen such that the constraints on paths through the target hold (constraints declared in the same struct and in enclosing structs; constraints in descendants may not be enforced). If no element satisfies the constraints (or the list is empty) → generation contradiction. Not allowed under a `when` subtype; may not take part in an ICFS; pointer-equality or `like`-narrowing constraints on the target are unsupported.
**Work:** parser: `chosen_from` keyword in `keep` expressions (binary at the `in` row); the right side must be `read_only(exp)` → **`E0131`** otherwise. Gen: filter the list by evaluating the target's other constraints with each candidate substituted; pick uniformly with the seeded RNG; **`E0417`** "chosen_from: no element of 'lps' satisfies the constraints on 'p'".
**Runtime test:**
```e
<'
struct packet_s { x: uint; };
struct holder {
    !lps: list of packet_s;
    p: packet_s;
    keep p chosen_from read_only(lps);
    keep p.x > 5;
};
extend sys {
    run() is also {
        var h: holder = new;
        for i from 0 to 9 { var q: packet_s = new; q.x = i; h.lps.add(q); };
        for k from 1 to 20 { gen h.p; out(h.p.x > 5 and h.lps.has(it list_ptr== h.p) == FALSE or TRUE); };
    };
};
'>
```
Simplify if `has(it list_ptr== ...)` is not available: assert `h.p.x > 5` → 20 lines `TRUE`; then set every `x` to 1 and `gen h.p` → `E0417`.

---

## Phase 7 — Custom numeric types and precision rules **[Opus]**

### T7.1 `numeric_type` declaration and `implementing numeric_i of (...)` (5.8.4, 5.2.15, 31.4) (L)
**Syntax:** `numeric_type T: S;` where `S` is a struct declared `struct S [like ...] implementing numeric_i of (S, is-signed, bit-size) { ... }` (also `extend S implementing numeric_i of (...)`; template forms per 8.4). `bit-size` may be `UNDEF`. Range/bit modifiers may not be applied to a custom numeric type. A struct implementing `numeric_i` cannot have subtypes. Values of `T` have **value semantics** (not pointers) even though operations are struct methods.
**Semantics:** every operator/conversion on `T` dispatches to the corresponding `numeric_i` method of `S` (T7.2); an unimplemented method used at run time → runtime error.
**Work:** `DeclarationKind::NumericType`; `TypeExpr::Named` classification "custom numeric". Interp: a `Value::Custom { type, instance }` whose instance is a struct; copy-on-assign (deep copy via `numeric_assign`); binary operators look up the method table. **`E0132`** "numeric_type base must be a struct implementing numeric_i"; **`E0418`** "operation '+' is not implemented by numeric type 'fixed_t' (numeric_add missing)".

### T7.2 Predefined interfaces `base_numeric_i`, `numeric_i`, `numeric_format_kind` (Clause 31) (M)
Declare in `builtins.rs` as predefined (template) interfaces with these signatures (implementers see the *names*; the semantics of each are the obvious operator semantics — e.g. `numeric_add(other, res)` writes `me + other` into `res` after `res.set_initial_value()`):

`base_numeric_i`: `set_initial_value()`, `numeric_to_string(radix: numeric_format_kind): string`, `numeric_from_string(value: string)`, `numeric_to_longuint(): longuint`, `numeric_from_longuint(value: longuint)`, `numeric_to_longint(): longint`, `numeric_from_longint(value: longint)`, `numeric_to_unbounded(): int(bits:*)`, `numeric_from_unbounded(value: int(bits:*))`, `numeric_to_real(): real`, `numeric_from_real(value: real)`, `numeric_to_uint(): uint`, `numeric_from_uint(value: uint)`, `numeric_to_int(): int`, `numeric_from_int(value: int)`, `numeric_to_numeric(other: base_numeric_i)`.

`numeric_i of (<me'type>, <signed'exp>: bool, <bits'exp>: int)` like `base_numeric_i`: `numeric_assign(other)`, `numeric_compare(other): int` (-1/0/1), `numeric_bit_and/or/xor(other, res)`, `numeric_multiply/divide/modulo/add/subtract(other, res)`, `numeric_shift_right/left(n: int, res)`, `numeric_bit_not(res)`, `numeric_unary_minus/plus(res)`, `numeric_extract_bits(from: int, to: int, buffer: list of bit)`, `numeric_extract_bits_assign(from, to, values: list of bit)`, `numeric_floor/ceil/round(res)`, `numeric_log/log10(res)`, `numeric_pow(exponent, res)`, `numeric_sqrt/exp/sin/cos/tan/asin/acos/atan/sinh/cosh/tanh/asinh/acosh/atanh(res)`, `numeric_atan2(other, res)`, `numeric_hypot(other, res)`, `numeric_is_nan/is_finite/is_pos_inf/is_neg_inf(): bool`, `numeric_abs(res)`, `numeric_odd/even(): bool`, `numeric_ilog2/ilog10(res)`, `numeric_ipow(other, res)`, `numeric_isqrt(res)`, `numeric_div_round_up(other, res)`.

`numeric_format_kind` enum: `none, dec, hex, bin, oct, udec, float`.

**Runtime test (minimal fixed-point type):**
```e
<'
struct fx_s implementing numeric_i of (fx_s, TRUE, 32) {
    !raw: int;
    set_initial_value() is { raw = 0; };
    numeric_assign(other: fx_s) is { raw = other.raw; };
    numeric_from_int(v: int) is { raw = v * 100; };
    numeric_to_int(): int is { return raw / 100; };
    numeric_add(other: fx_s, res: fx_s) is { res.raw = raw + other.raw; };
    numeric_compare(other: fx_s): int is {
        if raw < other.raw then { return -1 } else if raw > other.raw then { return 1 } else { return 0 };
    };
    numeric_to_string(radix: numeric_format_kind): string is { return append(raw / 100, ".", raw % 100); };
};
numeric_type fx_t: fx_s;
extend sys {
    run() is also {
        var a: fx_t = 3;
        var b: fx_t = 4;
        var c: fx_t = a + b;
        out(c);
        out(c.as_a(int));
        out(a < b);
        var d: fx_t = a * b;
    };
};
'>
```
Expected: `7.0`, `7`, `TRUE`, then runtime `E0418` for `*`.

### T7.3 Operator dispatch and conversions (5.2.15, 5.7, 31.2) (M)
Implicit conversions use the `numeric_to_*`/`numeric_from_*` pairs per the context type; `print`/`append`/`dec()`/`hex()` call `numeric_to_string(radix)` with the matching `numeric_format_kind`. Covered by T7.2's test; add `out(hex(a))` once `numeric_to_string` honours `hex`.

### T7.4 Six-way precision rules incl. 64-bit (5.6.3) (M) — can be done before T7.1
**Spec (paraphrased):** determine the context type set (up to three types). (a) If every type is ≤ 32 bits: compute in 32 bits; unsigned if any operand is unsigned; decimal literals are signed, other literals unsigned unless preceded by `-`. (b) If any type is `longint`/`longuint` and the rest are ≤ 32 bits or 64-bit long types: compute in 64 bits; **unsigned if any is `longuint`**; operands are cast. (c) If any type (other than `longint`/`longuint`) is wider than 32 bits: compute in unbounded precision (`int(bits:*)`), sign/zero-extending operands. (d) If any type is `real`: compute in double precision, operands converted to real. Consequence: `int(bits:64) -1 < uint(bits:64) 1` is TRUE (rule c) while `longint -1 < longuint 1` is FALSE (rule b, -1 becomes `MAX_LONGUINT`).
**Work:** today the interpreter uses `i128` everywhere without truncation. Implement a `precision_context()` in `interp.rs` for binary operators and comparisons with the four modes; truncate results to 32/64 bits in modes (a)/(b); keep the existing unbounded path for (c). Document the remaining gap (32-bit wrap-around for intermediate results is now applied — update README "Integers are unbounded i128 (widths are not truncated)").
**Runtime test:**
```e
<'
extend sys {
    run() is also {
        var x1: int(bits: 64) = -1;  var x2: uint(bits: 64) = 1;
        out(x1 < x2);
        var y1: longint = -1;        var y2: longuint = 1;
        out(y1 < y2);
        var z1: real = 7.2;          var z2: int(bits: *) = 10;
        out(z1 + z2);
        var u: uint = 0;
        out(u - 1 == MAX_UINT);
    };
};
'>
```
Expected: `TRUE`, `FALSE`, `17.2`, `TRUE`.

---

## Phase 8 — Execution phases and predefined-library surface **[Sonnet]**

Each builtin here is implemented either for real (when the model exists) or as an **honest stub** that parses, type-checks its arity, and reports **`E0419`** "'name()' is recognised (clause X) but not implemented in this stage" at run time. Every stub must be listed in `docs/ROADMAP.md` under a "2019 library surface" heading with its status.

### T8.1 Phases: `extract()`, `check()`, `finalize()`, `rerun()` of `any_struct`; `sys.setup()` (Table 1, 28.3.3.6–28.3.3.8, 28.3.5.1) (M)
**Spec:** phase order: init → setup (`sys.setup()`) → generate (`pre_generate()`, `post_generate()`) → run (`run()`) → extract (`extract()`) → check (`check()`) → finalize (`quit()`, `finalize()`). In each of extract/check/finalize the method is called on all structs under `sys` (same DFS order as `run()`); all three are initially empty. `rerun()` deactivates a struct/unit at the end of the current tick via `quit()` and reactivates it at the start of the next cycle via `run()`.
**Work:** `interp.rs` run loop: after the scheduler drains, call `extract()` on every live instance (DFS), then `check()`, then `quit()`/`finalize()`; `expect`/`assume` monitoring and coverage grading move to the check phase. `rerun()` as a builtin on instances with the TCM scheduler.
**Runtime test:**
```e
<'
struct leaf { run() is also { out("leaf run"); }; extract() is also { out("leaf extract"); }; check() is also { out("leaf check"); }; finalize() is also { out("leaf finalize"); }; };
extend sys {
    l: leaf;
    setup() is also { out("setup"); };
    run() is also { out("sys run"); };
    extract() is also { out("sys extract"); };
    check() is also { out("sys check"); };
    finalize() is also { out("sys finalize"); };
};
'>
```
Expected order: `setup`, `sys run`, `leaf run`, `sys extract`, `leaf extract`, `sys check`, `leaf check`, `sys finalize`, `leaf finalize`.

### T8.2 Generation sub-phases `connect_pointers()`, `connect_ports()`, `check_generation()` (9.13.2–9.13.4) (M)
**Spec:** after the unit tree is generated and all `post_generate()` calls are done, `sys.connect_pointers()` runs and every unit under `sys` is processed depth-first (field order), each unit's `connect_pointers()` before moving on; then likewise `connect_ports()`; then `check_generation()`. All three are initially empty. `do_bind()` may only be called in the `connect_ports()` sub-phase (error otherwise); port values may not be read in `connect_ports()`; references assigned in `connect_pointers()` may not be used in constraints or `post_generate()`.
**Work:** interp: three DFS passes over units after implicit generation; `do_bind` outside `connect_ports` → **`E0420`**.
**Runtime test:** two units `a`, `b` under `sys`, each with `connect_pointers() is also { out("cp <name>") }`, `connect_ports() is also { out("cpt <name>") }`, `check_generation() is also { out("cg <name>") }` → output `cp a`, `cp b`, `cpt a`, `cpt b`, `cg a`, `cg b` (after `post_generate` outputs, before `run` outputs).

### T8.3 Port binding methods (9.13.5–9.13.12) (S, stubs where no port model)
`do_bind_unit(u1, u2)` (not symmetric; connect_ports sub-phase only), `p1.connect(p2)` (directed method-port connection; identical method types), `p.remap_hdl_path("…")`, `p.disconnect()`, `p.disconnect_bound_set()` (connect_ports sub-phase only), `p.get_bound_set(): list of any_port` (any time; a disconnected port returns only itself), `p.is_connected(): bool` (any time). Implement `get_bound_set`/`is_connected`/`disconnect*` over the existing `simple_port` + `keep bind` model if feasible; stub the rest with `E0419`. Test: two bound simple ports → `p.get_bound_set().size() == 2`, `p.is_connected() == TRUE`; a port with `keep bind(p, empty)` → size 1, `FALSE`.

### T8.4 Unit attributes and `any_unit` methods (7.3.4, 7.3.5, 7.4.4–7.4.7) (S)
`keep u.pack_options() == packing.low_big_endian` and `keep u.per_inst_cover() == DEFAULT|YES|NO` attribute constraints — record (like `hdl_path()`); `get_children([with_ports: bool = FALSE]): list of any_unit` (direct child unit instances in instance-field declaration order, lists flattened, NULLs skipped; with ports when asked), `get_ports(): list of any_port`, `get_ports_recursively()`. Test: `sys` with units `u1`, `u2: list of u (2)` → `sys.get_children().size() == 3`, `e_path()` order `sys.u1`, `sys.u2[0]`, `sys.u2[1]`.

### T8.5 `set_check(root_unit …)`, `set_check_by_name()` (16.3.6, 16.3.7) (S)
Add the `root_unit` parameter to the existing `set_check` builtin (if present) and a `set_check_by_name(name, severity|effect, root_unit)` stub. Parse/arity test only.

### T8.6 Numeric generalisation of list pseudo-methods (27.4.17, 27.4.20, 27.5.4–27.5.6) (S)
`max_value`, `min_value`, `average`, `product`, `sum` accept any numeric `exp` (real included) and return the type of `exp`; `average` returns `UNDEF` on empty, `product` 1, `sum` 0. Test: `{1.5; 2.5}.sum(it)` → `4.0`; `{1; 2; 4}.average(it)` → `2` (integer division keeps the int type); `{}.product(it)` → `1`.

### T8.7 `deep_copy()` generalisation (29.2.2) (S)
Scalars/strings/sets return the value; lists return a new list of deep copies; structs as today (and `copy()` of every copied struct is called). Test: `var l2 = deep_copy(l1); l2.add(9); out(l1.size(), l2.size())`.

### T8.8 `files.flush_log_file()` (30.5.16) (S) — stub `E0419`; parse test.

### T8.9 Coverage API additions and renames (14.8.2.10, 14.9.6–14.9.17) (M)
`covers.sample_cg("type.group")` — procedurally samples a cover group (and all its instances) without emitting its event; not permitted before `post_generate`. Implement on `src/coverage.rs` (the sampler already exists — trigger it by name). Renames: `covers.compute_overall_grade()` returns the normalised grade (1 M–100 M scale; the old `get_overall_grade()` now *reads* the last computed value), `covers.get_ucd_path()` replaces `get_ecov_name()`. Stubs: `set_per_inst_for_struct(struct, unit)`, `set_unicov_hier(inst, path)`, `set_cross_instance_collection(item, pattern, bool)`, `get_top_size_items(n)`. Test: a cover group on event `e` whose item is a `uint(bits:2)`; call `covers.sample_cg("s.cg")` three times after assigning 0,1,2 without emitting `e` → `run --coverage` reports 3 of 4 buckets hit.

### T8.10 Message hooks (24.9.2.14) (S) — `user_message_handler()`, `user_transaction_handler()`, `user_transaction_start_handler()` as empty, extendable hooks on `any_unit`; the `message_manager` helper methods of Tables 51–53 as `E0419` stubs. Test: `extend any_unit { user_message_handler() is also { out("hook") } }` plus one `message(LOW, "x")` → `hook` printed once.

### T8.11 `for each in set` over enum/bool sets (20.2.5) (S)
`for each (c) in [red..blue]` (enum set) iterates items in order. Test: prints `red green blue`.

### T8.12 `swap()` and the pack-options wording (19.1) (S) — ensure `swap()` is a recognised pack tool name (stub or implement); doc only.

---

## Phase 9 — Tables (Clause 26, 32.3) **[Opus]**

### T9.1 `table … with { … }` preprocessor expansion (26.2) (L)
**Syntax:** `table [count] table-data with { body-code }` is a preprocessor construct usable wherever a code block can appear (top-level statements, struct members, constraints, actions). `table-data` is one of:
1. inline rows `{ row; row; ... }` where a row is cell-values separated by `|`; a cell is arbitrary text without `;`/`|` unless inside parentheses/brackets; the first row may be a header whose cells are all `#name` (legal e names);
2. an inline constant set literal, e.g. `[0,4..7]` ≡ `{0;4;5;6;7}` (one column);
3. `from table-operator(params) [using option, ...]` (T9.2).
With `count`, the data is first reduced to a single-cell table whose value is the row count.
**Expansion:** the construct is replaced by `{ ... }` containing `body-code` once per data row, with each cell reference substituted: `<#n>` (column n, from 1), `<#name>` (header name), `<#0>` or `<#row_index>` (row index); in nested tables `<##n>`/`<##name>` refer to the enclosing table's row, `<###…>` two levels up, etc. The expansion must be legal code in its context.
**Work:** a second pre-pass in `src/macros.rs` (or a new `src/tables.rs`) run before macro expansion on the token stream; `table` is contextual (identifier followed by `{`, `[`, `from`, `count`). Diagnostics **`E0133`** "table row has N cells, header has M", **`E0134`** "unknown table column '<#foo>'", **`E0135`** "table cell reference outside a table body".
**Fixture/runtime test:**
```e
<'
struct regs {
    table { #name | #width ;
            ctrl  | 8      ;
            stat  | 4      ;
            data  | 16     } with {
        <#name>: uint(bits: <#width>);
        keep <#name> < <#width>;
    };
};
extend sys {
    run() is also {
        var r: regs; gen r;
        table [1..3] with { out("row <#0> value <#1>"); };
        table count { a | 1; b | 2 } with { out("rows: <#1>"); };
    };
};
'>
```
Expected: parse `OK`; `elab` lists fields `ctrl stat data` with widths 8/4/16; runtime output `row 0 value 1`, `row 1 value 2`, `row 2 value 3`, `rows: 2`.
Nested test: outer table over `{ A; B }`, inner `table [1..2] with { out("<##1>-<#1>") }` → `A-1 A-2 B-1 B-2`.

### T9.2 `table from operator(...) [using filter = exp, unique = exp]` and `csv_to_table()` / `excel_to_table()` (26.3) (M)
**Spec:** a table operator is a **final** method of `table_manager` whose return type is `list of list of string`, `list of table_row`, or `table`; parameters are scalar/string/set (or lists thereof) constants. Options: `filter = bool-exp` (rows evaluating FALSE are dropped), `unique = string-exp` (only the first row per distinct value is kept; applied after filter). Both may use `table_column("name-or-index")` to read the current row's cell. Predefined operators: `csv_to_table(file-name, tag)` (default extension `.csv`; the file must exist and contain a table with the tag) and `excel_to_table(file-name, sheet, tag)` (`.xls/.xlsx`).
**Work:** `csv_to_table` implemented with a tiny CSV reader honouring the 32.3.3.10–32.3.3.13 defaults (empty line = only whitespace/commas; a tag line is "text until the first comma"; tables end at an empty line); `excel_to_table` → **`E0419`** stub (no Excel reader; say so). **`E0136`** "table operator must be a final method of table_manager returning a table type".
**Test:** a fixture CSV under `tests/data/regs.csv`:
```
regs
name,width
ctrl,8
stat,4
dummy,2

```
and `table from csv_to_table("tests/data/regs", "regs") using filter = table_column("name") != "dummy" with { out("<#name>:<#width>") }` → `ctrl:8`, `stat:4`.

### T9.3 Procedural table API (32.3) (M)
Builtin types `table_manager` (singleton), `table`, `table_row` with: `create_table(column_names)`, `create_table_without_column_names(row_size)`, `create_empty_table()`, `create_row(size)`, `create_row_with_data(data)`, `read_table_from_csv(fname, tag, error_if_none)`, `read_all_tables_from_csv(fname)`, `read_table_from_excel`/`read_all_tables_from_excel` (stubs), `is_empty_line`, `is_table_end`, `skip_row`, `get_table_tag_from_row`; `table`: `get_csv_file_name`, `get_excel_file_name`, `get_excel_sheet_name`, `get_columns_num`, `get_column_name(i)`, `set_column_name(i, name)`, `get_header_row`, `replace_header_row`, `get_rows_num`, `get_row(i)`, `get_column_index_by_name`, `add_row`, `add_filled_row`, `insert_row`, `insert_filled_row`, `replace_row`, `delete_row`, `delete_column`, `copy`, `copy_with_rows`; `table_row`: `get_size`, `get_data(i)`, `set_data(i, s)`, `get_csv_line_num`, `get_csv_file_name`, `get_excel_file_name`, `get_excel_sheet_name`, `copy_source_info_from`, `copy`. Rows are numbered from 0, columns from 1 (column 0 = row index). A table read from a file is cached and read-only (modify → runtime error) — copy first.
**Test:** `var t := table_manager.create_table({"a";"b"}); t.add_row(); t.get_row(0).set_data(1, "x"); out(t.get_rows_num(), t.get_columns_num(), t.get_row(0).get_data(1))` → `1 2 x`.

---

## Phase 10 — Annotations (Clause 35) **[Opus design → Sonnet implement]**

### T10.1 Annotation type declaration (35.2) (M)
**Syntax:** `[package] [repeatable] annotation @name { member; ... }`. Semantics: a struct-like type inheriting from the predefined `annotation`; shares the type namespace; **restrictions:** cannot be extended or inherited; only fields and methods; fields only of scalar, string or set types, not `static`, no access modifiers; all fields are implicitly `const`; `init()` may not be extended; `new` is not allowed; a newly declared annotation type cannot be used in the same module (import cycle) — only from the next module.
**Work:** `DeclarationKind::Annotation { repeatable }`. Diagnostics **`E0137`** "annotation member must be a field or method", **`E0138`** "annotation field must be scalar/string/set", **`E0320`** "annotation type cannot be extended", **`E0321`** "annotation type used in the module that declares it".
**Fixture `s2019_annotation_decl.e`:**
```e
<'
annotation @development { developer: string; date: string; release: uint; };
repeatable annotation @reviewed { by: string; };
'>
```
Expected `OK`. Fixture with `extend development { }` → `E0320`.

### T10.2 Attaching annotations (35.3) (M)
**Syntax:** `@name(attr = const-exp, ...)` (parentheses optional when there are no attributes) immediately before: a type declaration/extension (`struct`, `unit`, `type`, `interface`, `numeric_type`, `method_type`, `extend`), a template declaration/extension, a struct member or interface member declaration/extension (fields, methods, events, expects, constraints, cover groups, `on` blocks), a macro definition, a C routine declaration. Not before a `when` block (error), not on a `table`. Several annotations may precede one entity; more than one of the same type only if `repeatable` → **`E0139`** "annotation '@x' is not repeatable". An annotation on `keep all of { ... }` attaches to each contained constraint. Annotations have **no** effect on semantics.
**Work:** `Annotation { name, attrs: Vec<(String, Expr)>, span }` attached to `Declaration`, `Member`, template decls, `MacroDecl`. Annotation in a macro body is ordinary replacement text; an annotation attached to a `<statement>`/`<struct_member>` macro call propagates to the definitions in the expansion (error if none).
**Fixture (two-module test via `import`):** `ann.e` declares `@development`; `s2019_annotation_use.e` has `import ann; @development(developer = "jo", release = 10) struct packet_s { @reviewed(by = "a") @reviewed(by = "b") x: uint; };` → `OK`; the `parse` dump shows the annotations.
Fixture `s2019_annotation_when.e` (annotation before `when`) → **`E0140`** "annotation cannot be attached to a when block".

### T10.3 Annotation API (35.4) (S stub / M if reflection stage allows)
Static methods auto-added to each annotation type: `@T.get_attached_annotation(element)` (non-repeatable only; returns the annotation or NULL), `@T.get_attached_annotations_list(element)`, `get_all_annotations()`; non-static `get_definition_element()`, hook `is_applicable(element): bool` (default TRUE; extend to restrict). `element` is an `rf_definition_element` — without the reflection model, implement `get_all_annotations()` for real (list of recorded annotations with their fields) and the others as `E0419` stubs; test `get_all_annotations().size()`.

---

## Phase 11 — Reflection catalogue as honest stubs **[Sonnet]**

The repo has no reflection model (only `set_of_values`). Do **not** build one here. Add a `docs/REFLECTION_2019.md` catalogue and `builtins.rs` names so that user code referencing them gets `E0419` with the clause number, not "unknown name". Catalogue to transcribe (names only — signatures as in §1.3):

- 32.1.2.3 `rf_object` (base of `rf_struct` and `rf_interface`), `rf_object.is_contained_in()`, `rf_object.is_instance_of_me()`.
- 32.1.2.5.2 `rf_struct_member.is_static()`; 32.1.2.5.4 `rf_range_generated_field` (`was_generated`, `was_reduced`, `get_range`, `rgf_assign_value`, `rgf_assign_value_unsafe`); 32.1.2.5.8–10 `rf_on_method`, `rf_on_event_method.get_triggering_event()`, `rf_on_event_path_method.get_triggering_event_string()`; 32.1.2.5.11 `rf_event.on_method()`; 32.1.2.5.13 `rf_check.get_check_actions()` / `get_relevant_check_actions()` (renamed).
- 32.1.2.6–7 `rf_interface` (`get_methods`, `get_declared_methods`, `get_method`, `get_base_interfaces`, `get_direct_sub_interfaces`, `get_all_sub_interfaces`, `get_implementors`), `rf_interface_member.get_declaring_interface()`, `rf_interface_method` (`get_result_type`, `get_parameters`, `get_implementing_methods`, `invoke`, `invoke_unsafe`); 32.1.2.8.5 `rf_like_struct.get_interfaces()`.
- 32.1.2.10 `rf_scalar.get_set_of_values()`/`get_full_set_of_values()` (moved from `rf_numeric`), `rf_scalar.get_bit_size_modified(bits)`, `rf_scalar.get_range_string()`, `rf_numeric.is_longint()`, `rf_custom_numeric.get_implementing_struct()`.
- 32.1.2.13 `rf_annotation`, `rf_non_repeatable_annotation`, `rf_repeatable_annotation`.
- 32.1.2.14 `rf_template_type`, `rf_template`, `rf_template_instance`, `rf_template_interface(_instance)`, `rf_template_numeric(_instance)`, `rf_template_parameter`, `rf_type_template_parameter`, `rf_value_template_parameter.get_type()`, `rf_template_instance_parameter.get_template_parameter()`, `rf_type_template_instance_parameter.get_type()`, `rf_value_template_instance_parameter.get_value()`.
- 32.1.2.17 `rf_manager.get_annotation_type_by_name`, `get_template_type_by_name`, `get_template_by_name`, `get_template_interface_by_name`, `get_user_template_types`, `get_user_templates`.
- 32.1.3 `rf_definition_element.get_source_lines_count()`, `rf_annotation.get_attached_annotations_list(element)`, `rf_non_repeatable_annotation.get_attached_annotation(element)`, `rf_struct_layer.get_added_interfaces()`/`get_exact_subtype()`, `rf_template_layer` (`get_added_interfaces`, `get_instance_layers`), `rf_template.get_layers()`, `rf_template_instance_layer.get_template_layer()`, `rf_module.get_actual_importer()`, `is_top_module()`, `get_full_file_name()`, `get_file_age()`.
- 32.1.4 `rf_type.value_to_type(value, to_type)`, `rf_manager.eval_exp_string(str)`, `rf_manager.insert_list_element[_unsafe]`, `delete_list_element[_unsafe]`, 32.1.4.6 `get_field_of_unit_instance(u)`, `get_list_indices_of_unit_instance(u)`, `get_list_index_of_unit_instance(u)`.
- 32.2 `instance_iterator` (`start`, `start_all`, `finish`, `next`, `get_current_instance`, `get_current_depth`, `get_current_path`, `get_path_item_by_index`, `get_current_path_item`, `get_current_path_string`, hooks `skip_field`, `skip_instance`, `with_ports`), `instance_path_item` (`get_instance`, `get_field`, `get_list_index`, `get_list_indices`), subtypes `unit_instance_iterator`, `unit_and_port_instance_iterator`, `port_instance_iterator`. **`instance_iterator` is worth implementing for real** (M): DFS over struct/unit instance fields, lists flattened, visited objects not revisited, only `any_struct` descendants, unit instances only via instance fields. Test: a tree `sys.a`, `sys.a.b`, `sys.lst[2]` → `get_current_path_string()` sequence `sys`, `sys.a`, `sys.a.b`, `sys.lst[0]`, `sys.lst[1]`.
- 32.3 table API (T9.3).

---

## Phase 12 — Final documentation and claims pass **[Sonnet]**

- `README.md` status paragraph: list what of 2019 is implemented vs recorded vs stubbed, in the repository's "honest staged claims" voice.
- `docs/ROADMAP.md`: add a "IEEE 1647-2019 migration" section mirroring the phases above with checkboxes; move items as they land.
- `docs/ARCHITECTURE.md`: new sections for static storage, interface dispatch, table pre-pass, annotations metadata, rgf sets.
- `docs/VERIFICATION.md`: one entry per landed phase; for constructs that EDA Playground's Specman supports, record reference runs (interfaces, static members, `in_table`, `evaluate`, tables, annotations, `longint` precision are all good candidates — the `local/` corpora may already contain some).
- Remove the "2016" wording everywhere (T0.1 re-check).

---

## 3. Deferred / explicitly out of scope for this migration

- TLM `interface_port … using context, prefix=` form (9.14.2), TLM sockets changes (9.15) — the repo has no TLM model.
- Excel reading for `excel_to_table()` / `read_table_from_excel()` — stub only.
- Messages/recording-configuration API bodies (24.9) beyond the three hooks.
- Coverage database (`ucd`) files; `get_ucd_path()` returns an empty string with a note.
- Encryption (Clause 34), semaphores (Clause 33) — unchanged in 2019 apart from headings.
- Full reflection model (Phase 11 is a catalogue + `instance_iterator` only).

---

## 4. Consolidated task list (copy into the issue as checkboxes)

```
Phase 0 — Groundwork [Sonnet]
- [ ] T0.1 Clause-number sweep 2016→2019 (table §1.4)                     S/M
- [ ] T0.2 Keyword table alignment with 2019 Table 8                      S
- [ ] T0.3 REFERENCES/README designate 2019                               S
Phase 1 — Lexical/syntactic foundation [Sonnet]
- [ ] T1.1  /* */ comments standard; span segments; no nesting (4.2.4)    S
- [ ] T1.2  Statement spanning code segments (4.2.3)                      S
- [ ] T1.3  Brace-grouped statements & members (4.3.2/4.3.3)              S/M
- [ ] T1.4  longint/longuint/nibble; no subtypes of long types (Table 17) M
- [ ] T1.5  Precedence Table 12 (4.6)                                     S
- [ ] T1.6  list_ptr== / !list_ptr= (4.11.4)                              S
- [ ] T1.7  in_table (4.11.8)                                             M
- [ ] T1.8  evaluate {…} (4.17.7)                                         M
- [ ] T1.9  typeof()/typeof_item() (5.9)                                  M
- [ ] T1.10 Subtype qualifiers in dot access (4.17.3)                     M
- [ ] T1.11 Field init values + new field prefix grammar (6.9)            M
- [ ] T1.12 extend global policy (4.4.2.2)                                S
- [ ] T1.13 Name resolution: literal global (4.4.5.3)                     S
Phase 2 — Static members [Opus design → Sonnet]
- [ ] T2.1 Static fields + :: (6.9.3, 4.17.4)                             L
- [ ] T2.2 Static methods + restrictions (17.2.2, 17.2.4, C8)             M
- [ ] T2.3 Static events, static on, on Type::event (11.1.4.1, 11.2)      M
- [ ] T2.4 Keyed-list key restrictions (5.2.11)                           S
Phase 3 — Interfaces [Opus]
- [ ] T3.1 interface declaration (6.13)                                   M
- [ ] T3.2 implementing on struct/extend; conflicts (6.3)                 M
- [ ] T3.3 Interface types, assignment, casting (5.2.9, 5.4.4.4, 5.10.2.4) L
- [ ] T3.4 Dynamic dispatch; missing-implementation runtime error         M
- [ ] T3.5 print/to_string of interface values                            S
Phase 4 — Templates rework [Opus]
- [ ] T4.1 Value parameters (8.5.2)                                       M
- [ ] T4.2 conditions {…} + categories (8.5.3)                            M
- [ ] T4.3 implementing in templates (8.2.3)                              S
- [ ] T4.4 template extend (8.6)                                          M
- [ ] T4.5 Derived defaults (8.5.4)                                       S
- [ ] T4.6 template interface / template numeric_type (8.3, 8.4)          M
- [ ] T4.7 Substitution into bases/interfaces (8.7)                       S
Phase 5 — Range-generated fields [Opus]
- [ ] T5.1 ~ modifier rules (6.9.7)                                       S
- [ ] T5.2 Reduced-set generation (10.3.13)                               L
- [ ] T5.3 rgf= (18.6)                                                    S
- [ ] T5.4 rgf_was_generated/rgf_get_range/rgf_was_reduced (29.13)        S
Phase 6 — chosen_from [Opus]
- [ ] T6.1 keep x chosen_from read_only(list) (10.5.8)                    L
Phase 7 — Custom numerics & precision [Opus]
- [ ] T7.1 numeric_type + implementing numeric_i (5.8.4, 31.4)            L
- [ ] T7.2 base_numeric_i / numeric_i / numeric_format_kind (Clause 31)   M
- [ ] T7.3 Operator dispatch & conversions                                M
- [ ] T7.4 Six-way precision rules incl. 64-bit (5.6.3)                   M
Phase 8 — Phases & library surface [Sonnet]
- [ ] T8.1  extract/check/finalize/rerun; sys.setup (Table 1, 28.3)       M
- [ ] T8.2  connect_pointers/connect_ports/check_generation (9.13.2–4)    M
- [ ] T8.3  Port binding methods (9.13.5–12)                              S
- [ ] T8.4  Unit attrs + get_children/get_ports (7.3.4–5, 7.4.4–7)        S
- [ ] T8.5  set_check root_unit; set_check_by_name (16.3.6–7)             S
- [ ] T8.6  Numeric list pseudo-methods (27.4/27.5)                       S
- [ ] T8.7  deep_copy generalisation (29.2.2)                             S
- [ ] T8.8  files.flush_log_file (30.5.16)                                S
- [ ] T8.9  Coverage API additions/renames (14.8.2.10, 14.9.x)            M
- [ ] T8.10 Message hooks (24.9.2.14)                                     S
- [ ] T8.11 for each in set over any scalar set (20.2.5)                  S
- [ ] T8.12 swap() recognised (19.1)                                      S
Phase 9 — Tables [Opus]
- [ ] T9.1 table … with {…} expansion (26.2)                              L
- [ ] T9.2 table from op(...) using filter/unique; csv_to_table (26.3)    M
- [ ] T9.3 table_manager/table/table_row API (32.3)                       M
Phase 10 — Annotations [Opus design → Sonnet]
- [ ] T10.1 annotation @name {…} declaration (35.2)                       M
- [ ] T10.2 Attaching @name(...) to entities (35.3)                       M
- [ ] T10.3 Annotation API (35.4)                                         S/M
Phase 11 — Reflection catalogue [Sonnet]
- [ ] T11.1 docs/REFLECTION_2019.md + E0419 stubs for all 32.1 names      M
- [ ] T11.2 instance_iterator implemented (32.2)                          M
Phase 12 — Docs/claims [Sonnet]
- [ ] T12.1 README/ROADMAP/ARCHITECTURE/VERIFICATION pass                 S/M
```

---

## 5. Diagnostic codes introduced by this plan (keep in sync with module docs)

| Code | Stage | Meaning |
|---|---|---|
| E0121 | parser | `in_table` row/tuple arity mismatch |
| E0122 | parser/check | `evaluate` result type cannot be inferred |
| E0123 | parser | `return`/`wait`/`sync` inside `evaluate` |
| E0124 | parser | `extend` of an interface (unless found legal) |
| E0125 | parser | interface member is not a method signature |
| E0126 | parser | template value parameter without scalar type |
| E0127 | parser | `template extend` parameter count mismatch |
| E0128 | parser | `template extend` with no parameter entries |
| E0129 | parser | defaulted template parameter followed by non-defaulted |
| E0130 | parser/check | `rgf=` on a non-range-generated field |
| E0131 | parser | `chosen_from` without `read_only(...)` |
| E0132 | parser/check | `numeric_type` base is not a `numeric_i` implementor |
| E0133 | tables | table row/header cell count mismatch |
| E0134 | tables | unknown table column reference |
| E0135 | tables | cell reference outside a table body |
| E0136 | tables | invalid table operator |
| E0137 | parser | annotation member is not a field/method |
| E0138 | parser | annotation field of illegal type |
| E0139 | parser | repeated non-repeatable annotation |
| E0140 | parser | annotation on a `when` block |
| E0207 | check | subtype of `longint`/`longuint` |
| E0208 | check | `typeof()` cannot determine the static type |
| E0209 | check | `typeof_item()` of a non-list |
| E0211 | check | init value on a generated field |
| E0212 | check | `static const` without init value |
| E0213 | check | `!` combined with `~` |
| E0214 | check | range-generated `const` field |
| E0215 | check | `me` in a static method |
| E0216 | check | forbidden action in a static method |
| E0217 | check | (reserved; see E0309) |
| E0218 | check | illegal keyed-list key |
| E0219 | check | conflicting inherited interface signatures |
| E0224 | check/elab | coverage option/range/weight parameters, source-graph order/names/arity/size, scalar/derived effect-predicate scope or per-unit-instance scope (post-migration qualification) |
| E0304 | elab | static member in `when` subtype |
| E0305 | elab | static field physical/generated |
| E0306 | elab | static field of unit-instance type |
| E0307 | elab | `T::f` where `f` is not a static field declared in `T` |
| E0308 | elab | static member added to `global` |
| E0309 | elab | static method extended outside its struct |
| E0310 | elab | static event in `when` subtype |
| E0311 | elab | struct implements interfaces with conflicting signatures |
| E0312 | elab | struct assigned to an interface it does not implement |
| E0313 | elab | template value actual not a constant of the right type |
| E0314 | elab | template instance violates `conditions` |
| E0315 | elab | template type actual not in category |
| E0316 | elab | range-generated field not scalar |
| E0317 | elab/gen | unsupported constraint shape on rgf field |
| E0318 | elab | (retired by T4.4: the template-extension parameter-count mismatch reports as E0127; E0318 was the pre-plan allocation, moved from a pre-plan E0304 use by T2.1) |
| E0320 | elab | annotation type extended/inherited |
| E0321 | elab | annotation type used in its declaring module |
| E0411 | runtime | `list_ptr==` operand types differ |
| E0412 | runtime | `in_table` element type mismatch |
| E0413 | runtime | qualified field access on non-matching subtype |
| E0414 | runtime | interface method not implemented by actual struct |
| E0415 | runtime | `rgf=` value outside generated range |
| E0416 | runtime | rgf query/assign before generation |
| E0417 | runtime | `chosen_from` contradiction |
| E0418 | runtime | numeric_i method missing for an operation |
| E0419 | runtime | recognised 2019 library name, not implemented in this stage |
| E0420 | runtime | `do_bind()` outside `connect_ports()` sub-phase |

---

## 6. PR checklist (copy into each PR description)

- [ ] Branch from `main`; one phase (or one task for L/XL) per PR.
- [ ] `cargo fmt --all -- --check` ✔ · `cargo clippy --all-targets -- -D warnings` ✔ · `cargo test --all-targets` ✔ (paste the summary line).
- [ ] New fixtures `tests/fixtures/s2019_*.e` + `.expected`; new runtime tests in `tests/interp.rs` (or a new `tests/s2019.rs`).
- [ ] README status, ROADMAP checkbox, ARCHITECTURE section, CLI help updated in the same PR.
- [ ] Diagnostic codes registered in the module doc header and in §5 of the issue.
- [ ] No text from the IEEE standard pasted; clause numbers cited are **2019** numbers.
- [ ] `docs/VERIFICATION.md` entry added (what was run, what was observed; Specman reference run link if any).
