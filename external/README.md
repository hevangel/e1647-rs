# External references (`external/`)

Pinned, read-only upstream repositories used as lawful references while
developing e1647-rs. They are **not** part of the e1647-rs build; nothing in
`src/` links to or vendors them. Read them for syntax cross-checks, grammar
ideas, methodology patterns and example programs.

Rules:

- Treat every pin as read-only. Do not modify files under `external/`.
- Respect each pin's license as recorded below. "no license file" means
  *all rights reserved by the upstream author* — use it as a reference for
  **facts about the language** (keywords, syntax shapes), never as code to
  copy.
- Do not bump pins as routine cleanup; pin updates need a reviewed pull
  request describing the upstream changes.
- Do not copy example or test code from unlicensed pins into this repository.

## Pins

| Path | Upstream | License | Used for |
| --- | --- | --- | --- |
| `specman-e-parser` | [k-sparrow/specman-e-parser](https://github.com/k-sparrow/specman-e-parser) | MIT | Independent C++ parser for Specman e; its `specman.flex` keyword/operator table is our cross-check for the e lexical grammar |
| `uvm-ml-open-architecture` | [doswellf/combinator-uvm](https://github.com/doswellf/combinator-uvm) | Apache-2.0 (the vendored `uvm_ml/1.3/UVM_ML-1.3/ml` tree carries Cadence/AMD Apache-2.0 headers; the surrounding repository itself has no license file) | UVM-ML Open Architecture distribution, including `ml/adapters/uvm_e/` Specman e adapter sources — reference for e-side integration APIs |
| `vscode-specman-grammar` | [tsvi/specman-vscode-grammar](https://github.com/tsvi/specman-vscode-grammar) | MIT | TextMate grammar covering e syntax — cross-check for keyword sets and syntax categories |
| `vscode-specman-support` | [ErickSancho/vscode-specman-support](https://github.com/ErickSancho/vscode-specman-support) | MIT | Editor grammar and snippet shapes for e |
| `specman-e-support` | [k-sparrow/specman-e-support](https://github.com/k-sparrow/specman-e-support) | no license file (reference only, do not copy) | Second editor grammar; keyword list cross-check |

## OVM-e methodology library — status: no lawful public pin

The OVM-e (Open Verification Methodology for _e_) library was distributed
through Cadence/Verisity channels (the former `ovmworld.org` is gone). We
could not identify a canonical, lawfully licensed public Git repository for
it, so **no OVM-e submodule is pinned here**. Lawful ways to see OVM-e-style
e code:

- The `uvm_e` adapter and examples inside the pinned UVM-ML tree above.
- OVM-e library code that ships with Specman installations (requires a
  Specman license).
- OVM-e-flavored examples on EDA Playground (run under the account configured
  in `.env`; see [docs/REFERENCES.md](REFERENCES.md)).

If a suitable upstream appears, propose it in an issue before pinning.
