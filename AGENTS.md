# AI Agent Guidelines — e1647-rs

## Role

You are the sole author of all code, documentation, data and configuration in
this repository. Humans never edit files by hand; they direct the work and
review, approve or reject pull requests. Follow
[CONTRIBUTING.md](CONTRIBUTING.md) for branches, verification and pull
requests.

This repository is an experimental, independent Rust implementation of tooling
for the IEEE 1647 _e_ verification language. It is staged on purpose: a claim
of support must always match what the code and its tests actually do. The
owning GitHub issue for the current qualification effort is
`hevangel/e1647-rs#1`.

## Required reading

Read the documents that cover your task before changing anything:

| Task | Read |
| --- | --- |
| Any change: scope, workflow, verification, lawful-reference rules | this file, [CONTRIBUTING.md](CONTRIBUTING.md) |
| What is implemented and what is deliberately not | [README.md](README.md), [docs/ROADMAP.md](docs/ROADMAP.md) |
| Design and stage boundaries | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |
| IEEE 1647 access, Specman reference runs, external references | [docs/REFERENCES.md](docs/REFERENCES.md), [external/README.md](external/README.md) |

## Essential rules

- Work in a feature branch and merge through a reviewed pull request; never
  push directly to `main`.
- **Honest staged claims.** Never state or imply support that does not exist.
  Tokenizing is not parsing; parsing is not type checking; type checking is
  not elaboration; none of these are simulation. Keep the README status line,
  roadmap checkboxes and CLI help accurate in the same change that moves them.
- **Lawful references only.** Do not commit, paste or redistribute any part of
  the copyrighted IEEE 1647 standard text, Specman/proprietary output dumps,
  or proprietary library sources. Reference the standard by clause, cite it,
  and keep any lawfully downloaded copy under `local/` (gitignored). Tests
  must be independently authored; do not copy test suites from proprietary
  distributions or from unlicensed repositories.
- **External references are read-only.** `external/` holds pinned upstream
  repositories used as syntax/semantics references and examples. Read them,
  do not modify them, do not vendor their files into `src/`, and respect each
  pin's license (see [external/README.md](external/README.md)). Do not bump
  pins as routine cleanup.
- **Secrets.** EDA Playground credentials live only in `.env` (gitignored;
  see `.env.example`). Read them from the environment, never hard-code them
  in source, tests, examples, docs, skills, logs or screenshots. If a command
  would echo the password, redact it.
- **Verification.** Before opening a pull request run, and report the exact
  results of:
  ```sh
  cargo fmt --all -- --check
  cargo clippy --all-targets -- -D warnings
  cargo test --all-targets
  ```
  New language behavior needs fixture tests under `tests/fixtures/` with
  expected diagnostics. Report exactly which checks you ran; do not claim a
  check passed unless it did.
- **Reference-simulator evidence.** When qualifying behavior against Cadence
  Specman on EDA Playground, record what was run and what was observed in
  [docs/VERIFICATION.md](docs/VERIFICATION.md). Reference runs show expected
  behavior; they never authorize copying vendor output or code.
- This repository may itself be pinned as a submodule of `hevangel/ai_engineer`.
  Changes are merged here first; the parent repository updates its pin in a
  separate reviewed pull request.
