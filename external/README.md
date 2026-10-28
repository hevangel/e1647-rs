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
| `design-verification` | [uobdv/Design-Verification](https://github.com/uobdv/Design-Verification) (pinned `0a48307`) | no license file (reference only, do not copy) | `SN-CPU-Tutorial/cpu/e/gold` — the golden Specman CPU verification environment (signal map, driver, refmodel, checker, tests) used as a runnable-goal corpus and construct-inventory source |
| `spmn-e-utils` | [efratcdn/spmn-e-utils](https://github.com/efratcdn/spmn-e-utils) (pinned `1fa3555`) | no license file (reference only, do not copy) | Index of small e utility examples (the utilities live in sibling `efratcdn/*` repos); used to source additional real-world e files for front-end testing — see "Testbench corpora" below |

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

## Testbench corpora (in `local/`, gitignored)

Real-world e testbenches used to measure the front-end are kept **out of
the repository** under the gitignored `local/` folder — third-party code
is never committed. Provenance for each corpus:

| Local folder | Source | Contents | Baseline |
| --- | --- | --- | --- |
| `local/eda-examples/` | EDA Playground curated "Specman e + SV/Verilog" playgrounds: [/x/Pfq](https://www.edaplayground.com/x/Pfq) (XOR TCM example), [/x/D6W](https://www.edaplayground.com/x/D6W) (Hello e World), [/x/5Wj](https://www.edaplayground.com/x/5Wj) (SER UVC), [/x/ZFqZ](https://www.edaplayground.com/x/ZFqZ) (UART verification) | 4 testbenches + designs | docs/VERIFICATION.md entry (2026-10-27, "curated Specman-e examples: baseline") |
| `local/asic-world/` | [asic-world Specman examples](https://www.asic-world.com/examples/specman/index.html) (memory/fifo/uart multi-file projects; arbiter is stub-only upstream) | 24 extracted code blocks | docs/VERIFICATION.md entry 7 |
| `local/sn-cpu-tutorial/` | Concatenated `SN-CPU-Tutorial/cpu/e/gold` files (from the `design-verification` pin above) — `golden-combined.e`, the runnable-goal corpus | 796-line combined golden testbench | docs/VERIFICATION.md entries 8–10 |
| `local/spmn-utils/` | Shallow clones of **all** utility repos under [efratcdn](https://github.com/efratcdn) indexed by the `spmn-e-utils` pin plus the remaining sibling repos: `cover_callback`, `cover_cb_report_hit`, `cover_cb_report_progress`, `cover_on_any_event`, `dependency_util`, `e-utils`, `e2hdl_check_alignment`, `e_type_min_max`, `etl`, `hello-world`, `igen-dist`, `keep_multi_lists`, `list_mult_delete`, `parallel_for_each`, `reflection_example`, `reflection_examples`, `save_restart_demo`, `spmn-dut_error`, `type_min_max` (empty) | 53 e files across 19 repos (libraries + usage examples; no license files upstream — reference only, do not copy) | docs/VERIFICATION.md entries 11–12 |
| `local/amiq-uvm-ref-flow/` | [amiq-eda/example-projects-uvm_ref_flow_1.1](https://github.com/amiq-eda/example-projects-uvm_ref_flow_1.1) `soc_verification_lib` (UVM-e reference flow, v1.1 2012) | 166 parseable e files: apb/ahb/uart/spi/gpio UVCs, apb_subsystem, uart_ctrl (`uart_g5.e` is binary content, excluded); Apache-2.0 overall, LGPL-3.0 for the bundled OpenCores designs | docs/VERIFICATION.md entry 12 |

Rules for these corpora mirror the pin rules: read-only measurement
input, never committed, never copied into `src/` or `tests/`. Our own
independently authored examples live in `examples/` and are the only
files that ship with the repository.
