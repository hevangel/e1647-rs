# References and lawful access

This repository implements tooling for the IEEE 1647 _e_ language using
independently written Rust code. This page records where references come from
and the rules that keep the repository free of redistributed copyrighted
material.

## IEEE 1647 standard

- **Designation:** IEEE 1647-2019 — *IEEE Standard for the Functional
  Verification Language e* (revisions 1647-2006/2008/2011/2016 exist; 2019 is
  current).
- **Official page:** <https://standards.ieee.org/ieee/1647/7193/>
- **Access:** The standard is copyrighted by IEEE. It is available through
  IEEE Xplore and the [IEEE GET Program](https://standards.ieee.org/products-programs/ieee-get-program/)
  (Design Automation standards) for free download for personal use under
  IEEE's terms — subject to accepting IEEE's license conditions. **This
  repository never redistributes the standard or quotes it at length.**
  Reference semantics by clause number and description, and cite the clause.
- **Local copies:** An agent or maintainer who lawfully downloads the PDF may
  store it under `local/` (gitignored, never committed) for personal study
  while developing tests. Commit summaries may cite clause numbers but must
  not reproduce specification text.

## Cadence Specman via EDA Playground

- [EDA Playground](https://www.edaplayground.com) provides hosted access to
  Cadence Specman (among other simulators) under its own terms of service.
  It is our authorized reference for observing real Specman behavior for
  language constructs we are implementing.
- Login credentials are stored **only** in the gitignored `.env` file (see
  `.env.example`). Read them from the environment; never write them into
  source, tests, docs, skills, logs or screenshots.
- The `edaplayground` agent skill (workspace skills directory) documents how
  to drive the site with the built-in browser.
- Recording results: log what was run and what was observed in
  [docs/VERIFICATION.md](VERIFICATION.md). Reference runs demonstrate expected
  behavior; they never authorize copying vendor output into tests. Expected
  values in tests must be independently derived from the language semantics.

## Open-source references (`external/`)

See [external/README.md](../external/README.md) for the pinned repositories,
their licenses and usage rules. The e keyword and operator set implemented in
`src/lexer.rs` is cross-checked against the MIT-licensed
`external/specman-e-parser/specman.flex` and the MIT-licensed editor grammars,
which encode the language's lexical facts independently of the IEEE text.

## Trademarks

Specman® is a registered trademark of Cadence Design Systems, Inc. IEEE is a
trademark of the IEEE. This project is independent and unaffiliated; mentions
are descriptive only.
