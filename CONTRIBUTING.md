# Contributing to e1647-rs

## The One Rule

**All changes to this repository must be made by an AI coding agent.** Human
contributors do not edit files directly — not code, not documentation, not
data, not configuration. If you want something changed, instruct your AI agent
to make the change.

This is not a suggestion. Pull requests containing hand-written edits will be
rejected.

Humans contribute by:

- Defining requirements and filing issues
- Directing AI agents
- Reviewing pull requests
- Approving or rejecting changes
- Providing feedback for the agent to address

## How to Contribute

### 1. Clone

```sh
git clone https://github.com/hevangel/e1647-rs.git
cd e1647-rs
git submodule update --init external/
```

### 2. Create a Branch

Never commit directly to `main`.

```sh
git checkout -b <type>/<description>
```

Branch naming:

- `agent/<topic>` — implementation work by an AI agent
- `feature/<description>` — new language or CLI capability
- `fix/<description>` — bug fixes
- `docs/<topic>` — documentation only

### 3. Instruct Your AI Agent

Tell your agent what you want. The agent reads [AGENTS.md](AGENTS.md),
[docs/ROADMAP.md](docs/ROADMAP.md) and [docs/REFERENCES.md](docs/REFERENCES.md)
for conventions, scope boundaries and lawful-reference rules.

### 4. Verify

The agent must run, and report the results of:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

Changes to language behavior additionally require fixture tests under
`tests/fixtures/`. See [docs/VERIFICATION.md](docs/VERIFICATION.md) for the
verification log and reference-simulator evidence.

### 5. Open a Pull Request

The pull request description must include:

- **What**: Clear description of the change
- **Why**: Motivation or issue reference
- **Verification**: The exact checks run and their results, plus any limitations
- **Agent**: Which AI agent authored the changes

### 6. Review Process

1. Maintainers review the pull request (humans review, not edit)
2. If changes are needed, the reviewer comments with feedback
3. The contributor's AI agent addresses the feedback
4. Repeat until approved
5. A maintainer merges

## What We Don't Accept

- Human-authored file edits (enforced by review)
- Copied proprietary source, vendor documentation text, or IEEE 1647 standard
  text (the standard is copyrighted; reference it by clause and cite it)
- Support claims broader than the tests demonstrate
- Secrets in any committed file (credentials belong in gitignored `.env`)
- Routine submodule pin bumps under `external/` without review of the
  represented upstream changes

## Issue Reporting

File issues with a clear title, the expected and actual behavior, and a
minimal reproducer. Please state whether the behavior is specified by IEEE
1647 (cite the clause), a documented extension, or an assumption.

## License

By contributing, you agree that your contributions are licensed under the
same terms as the project (see [LICENSE](LICENSE), Apache-2.0).
