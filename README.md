# e1647-rs

**An experimental, open-source Rust implementation of tooling for the IEEE 1647 _e_ hardware verification language.**

> **Status: pre-alpha.** Implements a source tokenizer and a parser for a
> declared subset of the language: struct/unit/extend/type declarations,
> fields, scalar/list/named and enumeral types, `keep`/`keep soft`
> constraints and expressions, `when` subtype members, and methods with
> parameter lists and statement bodies (`var`, assignment, calls, indexing,
> `return`, braced `if`/`else`, `while`/`break`/`continue`, `print`,
> `gen`), with diagnostic fixtures; plus member composition that folds
> `like` chains and `extend` blocks into per-type member lists. It does
> **not** yet do full type checking, `when`-subtype resolution, statement
> execution, constraint solving, randomization, temporal evaluation,
> coverage, HDL bridging or testbench execution. No IEEE conformance is
> claimed. See [docs/ROADMAP.md](docs/ROADMAP.md) for the staged plan.

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
```

## Repository layout

| Path | Contents |
| --- | --- |
| `src/` | The Rust crate (`lexer`, `ast`, `diagnostic`, `parser`, `check`, `elab`) and the `e1647` CLI |
| `tests/` | Integration tests and `tests/fixtures/` diagnostic fixtures |
| `examples/` | Small e-language sources used by tests and the CLI |
| `docs/` | Architecture, roadmap, references and the verification log |
| `external/` | Pinned read-only reference repositories (see [external/README.md](external/README.md)) |

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
Reference rules and lawful access to the IEEE 1647 standard are described in
[docs/REFERENCES.md](docs/REFERENCES.md).

## Standards and trademarks

This is an independent project targeting the IEEE 1647 _e_ language. The IEEE standard is copyrighted and must be obtained through appropriate channels; this repository does not redistribute the standard or proprietary Specman software.

**Specman®** is a registered trademark of Cadence Design Systems, Inc. This project is not affiliated with, sponsored by, or endorsed by Cadence Design Systems, Inc. References to Specman, if any, are descriptive only.

## License

Licensed under [Apache-2.0](LICENSE).

Contributions should be independently authored, without copying proprietary compiler/runtime code or proprietary verification libraries.
