# e1647-rs

**An experimental, open-source Rust implementation of tooling for the IEEE 1647 _e_ hardware verification language.**

<p align="center">
  <img src="slopman-logo.png" alt="Slopman: the e wordmark dripping green slop, representing the current state of this repository" width="240">
  &nbsp;&#8594;&nbsp;
  <img src="rustman-logo.png" alt="Rustman: the same e wordmark, clean, representing this repository once enough tokens have been burned" width="240">
  <br>
  <em>Slopman (present) &#8594; Rustman (pending sufficient token expenditure).</em>
</p>

> **Status: pre-alpha — full inventory in [docs/STATUS.md](docs/STATUS.md).**
> The implementation target is **IEEE 1647-2019**. Today the tool
> tokenizes, parses and elaborates a declared subset of the language and
> executes it: `e1647 run` generates seeded-random data under `keep`
> constraints, drives the Table 1 execution phases, runs TCMs on the
> clause 13 scheduler, monitors temporal `expect`/`assume` rules and
> collects coverage. The IEEE 1647-2019 feature set runs: static
> members, interfaces, the template rework, range-generated fields,
> `chosen_from`, custom numeric types, tables, annotations, the clause
> 33 resource-sharing structs (`semaphore`/`rdv_semaphore`/`locker`),
> and the reflection catalogue with `instance_iterator`. A full type
> system (a conservative definite-mismatch check includes coverage items),
> event-specific temporal timelines, general constraint solving and general testbench
> execution do **not** exist yet, and no
> IEEE conformance is claimed.

Temporal rules and waits retain sampled proposition and repeat-bound
values, so later state matching uses the recorded values. Temporal
`change`/`rise`/`fall` also sample Boolean, integer and enumeral
expressions; general event-specific timelines remain incomplete.

## From Slopman to Rustman

In keeping with this repository's policy of honest staged claims, the
mascot above is a status report. Everything here is authored by AI coding
agents under human review, and until the qualification corpus is complete
the honest word for that output is *slop*. Slopman wears his without
embarrassment: he is what this repository currently is — plausible, usefully
wrong in documented ways, and improving with every token.

He is not a permanent appointment. Every reviewed pull request, every
diagnostic fixture that earns its expected output, and every reference run
recorded in [docs/VERIFICATION.md](docs/VERIFICATION.md) burns more tokens
and drains a little more green from the *e*. No date is promised — only the
mechanism: work the staged plan in [docs/ROADMAP.md](docs/ROADMAP.md), keep
every claim matched by its tests, and one day, when enough tokens have been
burned, Slopman will quietly become Rustman. Same letter; less slime. Until
then, no conformance is claimed, and no cleanliness either.

## Goals

- Implement the language from independently developed Rust source code.
- Develop a reproducible conformance corpus for syntax, elaboration, random generation, temporal semantics, and coverage.
- Provide a usable CLI first, with an embeddable library and eventual WebAssembly target.
- Eventually integrate with open-source HDL simulators (including SystemVerilog and VHDL backends) via an explicit simulation API.

## Quick start

Requires a recent stable Rust toolchain supporting edition 2024.

```sh
cargo test
cargo run -- tokens examples/packet.e   # tokenize (does not parse)
cargo run -- parse examples/packet.e    # parse and print the AST or diagnostics
cargo run -- elab examples/methods_types.e  # fold like/extend, print the types
cargo run -- run --seed 1 examples/reference_subset.e  # execute sys.run()
```

With Docker, no local Rust toolchain is needed, and the pumpkin solver
feature can be verified on machines without a C compiler:

```sh
docker compose build               # build the toolchain image
docker compose run --rm verify     # fmt + clippy + tests, all features
docker compose run --rm demo       # run and elaborate the example programs
docker compose run --rm mixed      # mixed e + SystemVerilog testcase (xezim)
docker compose run --rm toolchain  # interactive shell in the container
```

`run` executes the built-in `sys` unit's `run()` methods. Every run takes
an explicit `--seed` and is fully deterministic per backend. By default
`gen` is a rejection sampler; translatable `keep` constraints solve
instead — linear ones through the backend conjunction solver (the
`solver-pumpkin` cargo feature), non-linear polynomial ones through a
feature-independent bounded enumeration solver — and everything else
falls back to rejection sampling (see `src/interp.rs` for the exact
semantics and its honest limits).

## Repository layout

| Path | Contents |
| --- | --- |
| `src/` | The Rust crate (`lexer`, `ast`, `diagnostic`, `parser`, `check`, `elab`, `builtins`, `interp`, `solve`, `hdl`) and the `e1647` CLI |
| `c/` | The VPI control bridge for mixed e + SystemVerilog testcases (simulated by xezim; see `examples/mixed_counter.e`) |
| `Dockerfile`, `compose.yml` | The toolchain container (see Quick start) |
| `tests/` | Integration tests and `tests/fixtures/` diagnostic fixtures |
| `examples/` | Small e-language sources used by tests and the CLI |
| `docs/` | Architecture, roadmap, status inventory, references and the verification log |
| `external/` | Pinned read-only reference repositories (see [external/README.md](external/README.md)) |
| `slopman-logo.png`, `rustman-logo.png` | The project mascot, present and prospective (see [From Slopman to Rustman](#from-slopman-to-rustman)) |

## Working on this repository

**Humans do not edit files here.** All changes are made by AI coding agents
and land through reviewed pull requests — see
[CONTRIBUTING.md](CONTRIBUTING.md). Agents follow
[AGENTS.md](AGENTS.md).

Verification before any pull request:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

Reference runs against the Cadence Specman implementation use
[EDA Playground](https://www.edaplayground.com). Credentials are read from a
gitignored `.env` file (copy `.env.example`); they must never be committed.
The automation playbook (login, run setup, reading logs, saving) lives at
[skills/edaplayground/SKILL.md](skills/edaplayground/SKILL.md); reference
rules and lawful access to the IEEE 1647 standard are described in
[docs/REFERENCES.md](docs/REFERENCES.md).

## Standards and trademarks

This is an independent project targeting the IEEE 1647 _e_ language. The IEEE standard is copyrighted and must be obtained through appropriate channels; this repository does not redistribute the standard or proprietary Specman software.

**Specman®** is a registered trademark of Cadence Design Systems, Inc. This project is not affiliated with, sponsored by, or endorsed by Cadence Design Systems, Inc. References to Specman, if any, are descriptive only.

## License

Licensed under [Apache-2.0](LICENSE).

Contributions should be independently authored, without copying proprietary compiler/runtime code or proprietary verification libraries.
