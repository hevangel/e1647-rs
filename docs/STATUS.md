# Status

**Pre-alpha.** The implementation target is **IEEE 1647-2019** — code
comments, diagnostics and docs cite 2019 clause numbers (historical
verification-log entries cite 2016; see [REFERENCES.md](REFERENCES.md)).
Everything below is backed by tests (`cargo test --all-targets`) and the
verification log; nothing here claims IEEE conformance. This page says
what runs today — the staged plan is [ROADMAP.md](ROADMAP.md), the
designs behind the claims are [ARCHITECTURE.md](ARCHITECTURE.md).

## The pipeline, end to end

- **Front end.** A source tokenizer and a recursive-descent parser for a
  declared subset: struct/unit/extend/type/interface/annotation and
  template declarations, fields, scalar/list/named/enumeral types,
  `keep`/`keep soft` constraints, `when` subtype members, ports, events,
  sequences, and methods with statement bodies. `import` loads a sibling
  file. Member composition folds `like` chains and `extend` blocks into
  per-type member lists. Failures produce spanned diagnostics with
  fixture-tested output (`tests/fixtures/`).
- **Elaboration.** The composed types feed the interpreter: `when`
  subtypes synthesize subtype views whose members apply;
  `typeof()`/`typeof_item()` type positions resolve; template instances
  monomorphize (actuals substitute for formals, method bodies included)
  and `template extend` blocks apply to the instances whose actuals
  match; static members register per type. After composition, the
  conservative type check ([`typing`]) walks every method body and
  reports definite mismatches only: a value that cannot be the target
  it is stored into (E0220), a member a known when-less receiver does
  not have (E0221), indexing a non-list (E0222), an operator on
  incompatible operand kinds (E0223) — anything uncertain stays
  silent.
- **Execution.** `e1647 run` executes the built-in `sys` unit through
  the Table 1 phases — `setup`, generation (with `pre_generate`/
  `post_generate` and the `connect_pointers`/`connect_ports`/
  `check_generation` sub-phases), the clause 13.2.2 scheduler running
  `start`ed TCMs concurrently, `run`, then `extract` → `check` →
  `quit`/`finalize`. `emit` feeds an event log; temporal waits
  re-evaluate against it; `expect`/`assume` rules monitor live — at
  every scheduler cycle boundary an invariant rule (`not`/`fail`)
  whose forbidden behavior occurred fires at that cycle, and the
  obligation rules (`=>`, `eventually`) re-check at quit when their
  windows close. The
  clause 33 blocking TCMs (`up()`/`down()`/`lock()`) yield at the call
  and park blocked calls on a per-object FIFO wake pass.
- **Randomization.** `gen` is a seeded rejection sampler by default;
  with the `solver-pumpkin` cargo feature it solves linear `keep`
  constraints instead — conjunctive, disjunctive (`||`, `not`, item-set
  membership via bounded branch enumeration) as well as plain
  conjunctions. `select` distributions
  (weights/min/max/edges/others/normal) bias the sampler; every run is
  deterministic per seed.
- **Observability.** Coverage groups (clause 14) collect basic-item
  buckets and grades on their sampling event (`run --coverage` reports
  them); `print`/`out` and the message hooks produce the run log.

## What runs, by 2019 feature

| Feature | Clause | Notes |
| --- | --- | --- |
| Standard `/* */` comments, segment-spanning statements, brace groups | 4.2.3, 4.2.4, 4.3.2–4.3.3 | non-nesting comments, statements across code segments |
| `longint` / `longuint`, the six-way precision rules | 5.2.3.1, 5.6.3 | 32/64/unbounded/real modes truncate where operand types pin them |
| `list_ptr==` / `!list_ptr=`, list literals, `in_table`, `evaluate`, `typeof` | 4.6, 4.11.4, 4.11.8, 4.17.7, 5.9 | lists carry reference identity |
| Subtype qualifiers in dot access (`p.red'good'x`) | 4.17.3 | resolved against the synthesized when views |
| Field initial values | 6.9.1–6.9.4 | `const`/`!`/`~` prefix grammar |
| Static fields, methods, events; `Type::member` | 6.9.3, 17.2, 11.1.4.1, 11.2 | per-type store; the 17.2.2 restrictions checked |
| Interfaces: `interface`, `implementing`, casts, dynamic dispatch | 6.13 | E0414 for a missing implementation |
| Templates rework: value parameters, `conditions`, `template extend`, derived defaults, template interfaces/numeric types | 8.2–8.7 | canonical instance names computed after binding |
| Range-generated `~` fields, `rgf=`, the rgf routines | 6.9.7, 10.3.13, 18.6, 29.13 | reduced-set generation, deterministic concrete value |
| `keep target chosen_from read_only(list)` | 10.5.8 | seeded uniform pick among satisfying elements |
| Custom numeric types + `numeric_i` dispatch | 5.2.15, clause 31 | values by value; missing `numeric_*` method is E0418 |
| Simple macros | 15 | expand before parsing |
| `repeat until`, the `do` sugar | 20 | |
| Tables: `table … with { … }`, `csv_to_table`, the `table_manager` API | 26, 32.3 | token pre-pass between `#define` and macro expansion |
| Annotations: `annotation @name`, attachments | 35 | recorded metadata, no semantic effect |
| `semaphore` / `rdv_semaphore` / `locker`, the resource-sharing TCMs | 33 | blocking `up()`/`down()`/`lock()` park on a FIFO wake pass; `try_up()`/`try_down()` never switch; `set_value`/`set_max_value` rules enforced |
| Reflection catalogue + `instance_iterator` | 32.1–32.2 | catalogue in [REFLECTION_2019.md](REFLECTION_2019.md); the base iterator walks the instance tree |
| Set/list pseudo-method subsets, `set_of_values` | 28.5, 27 | the expression-taking forms are the documented gaps |
| Simulator side | 23 | `rise/fall/change(HDL expression) @sim` events and `hdl_path()`-bound event ports sample each virtual cycle (a live xezim session, or an injected history); TCMs declared `@event` commence on the event |

Recognised-but-unimplemented 2019 library names (the full list sits in
`UNIMPLEMENTED_2019` in `src/interp.rs`) parse, arity-check, and report
**E0419** with their clause at run time instead of failing as unknown
names.

## Tests and the two revisions

The suite targets **2019 only** — there is no 2016 mode and no
per-revision test split. Coverage comes from two layers: the
pre-migration tests (authored against 2016-era behavior) pin the
**shared subset**, valid because the 2019 revision is almost purely
additive; the `tests/fixtures/s2019_*` fixtures and the migration's
tests pin the **2019-only additions**. The 2016-era corpus files under
the verification log's regression set (etl, cover-edges, the golden
testbenches) act as the legacy-behavior check — they were re-run on
every migration merge. Where a rule genuinely changed between the
revisions (the migration plan's §1.2 items: `extend global`'s wording,
bool in `set_of_values`, the numeric list aggregates, set iteration,
`deep_copy`'s generality, the phase naming, the 64-bit precision
rules), the pinning tests carry a "differential reading" comment
saying what 2016 did differently; the tool implements the 2019
reading, nothing else.

## What does not work yet

- A full type system (the [`typing`] conservative pass reports definite
  mismatches in method bodies; there is no general type inference, no
  name-resolution pass beyond it, and constraints/init values/`when`/
  `on` bodies are not walked).
- Streaming temporal evaluation (the engine re-matches each rule and
  wait over the finite run log at every cycle boundary; invariant
  rules fire live, but there is no incremental matcher following
  events as they occur, and obligation windows close only at quit).
- Non-linear constraint solving (`gen` handles the conjunctive and
  disjunctive linear subsets — `||`, `not`, item-set membership — via
  bounded branch enumeration; non-linear arithmetic is still unsolved;
  `chosen_from` and the rgf reduced sets are the sampling-side
  exceptions).
- General testbench execution (sequences run inline; the e↔HDL event
  ordering is not implemented; the VHDL backend is unwritten).
- The remaining out-of-scope list of the 2019 migration plan (TLM
  sockets, Excel readers, coverage-database files, message-manager
  tables, encryption, the full reflection model) — semaphores moved out
  of it when the clause 33 structs landed.

**No IEEE conformance is claimed.** Where this implementation's reading
of a clause is an approximation, the approximation is documented in the
nearest code comment and the PR that introduced it.
