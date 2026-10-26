# e1647-rs

**An experimental, open-source Rust implementation of tooling for the IEEE 1647 _e_ hardware verification language.**

> **Status: pre-alpha.** Currently implements a small source tokenizer and a `tokens` CLI command. It does **not** yet parse complete e programs, solve constraints, compile e code, or run testbenches. No IEEE conformance is claimed.

## Goals

- Implement the language from independently developed Rust source code.
- Develop a reproducible conformance corpus for syntax, elaboration, random generation, temporal semantics, and coverage.
- Provide a usable CLI first, with an embeddable library and eventual WebAssembly target.
- Eventually integrate with open-source HDL simulators (including SystemVerilog and VHDL backends) via an explicit simulation API.

## Quick start

Requires a recent stable Rust toolchain supporting edition 2024.

```sh
cargo test
cargo run -- tokens examples/packet.e
```

This command **tokenizes** the example; it does not execute it.

## Architecture

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and [docs/ROADMAP.md](docs/ROADMAP.md).

Initial milestones:

1. A lexer with precise source locations and regression fixtures.
2. Parser and typed AST for basic declarations, types, units, and expressions.
3. `extend` / `when` elaboration and constraints.
4. Constraint-solving interface and deterministic seeded generation.
5. Event scheduler, time-consuming methods, and HDL adapters.
6. Functional coverage, temporal expressions, and broader standards compatibility.

The roadmap is aspirational and does not imply feature support today.

## Standards and trademarks

This is an independent project targeting the IEEE 1647 _e_ language. The IEEE standard is copyrighted and must be obtained through appropriate channels; this repository does not redistribute the standard or proprietary Specman software.

**Specman®** is a registered trademark of Cadence Design Systems, Inc. This project is not affiliated with, sponsored by, or endorsed by Cadence Design Systems, Inc. References to Specman, if any, are descriptive only.

## License

Licensed under [Apache-2.0](LICENSE).

Contributions should be independently authored, without copying proprietary compiler/runtime code or proprietary verification libraries.
