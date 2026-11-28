# Explicit xezim VPI qualification profile

This profile exercises real SystemVerilog simulation through the native e
interpreter and the independently authored C VPI bridge. It is not a Specman
comparison or a claim of general IEEE 1647/1800 event ordering. The interpreter
still advances its own virtual scheduler separately from explicit `hdl_run`.

## Qualified observations

The runner checks xezim 0.11.3, revision `058f2bba` (the full source pin is
`058f2bba58648452e5799d17572356ebcae7d163` in Dockerfile). It checks:

- Explicit `hdl_load(design, top, "xezim")` selection in the positive profile
  and default backend selection in the two-argument counter examples.
- Signed 32-bit transfers, quoted paths and direct hdl_get/hdl_set calls.
- Three driven rising edges, settled counter values after explicit one-tick
  advances, zero-delay completion and repeated finish.
- Exact observations before and after zero-tick and one-tick steps. A clock
  edge driven by e triggers both immediate and one-tick-delayed nonblocking
  assignments in SystemVerilog. The trace checks the edge's simulation tick
  as well as the values seen by e.
- Both repository counter examples, including simple_port access, with exact
  five-cycle output. A printed MISMATCH cannot pass just because exit was zero.
- Missing signals, X/Z reads, wide signals, out-of-range writes and whitespace/
  newline paths, with exact E0408 diagnostics and nonzero exit codes.
- Raw bridge rejection of invalid integers/delays, trailing arguments and
  overlong command lines. Rejected writes leave the initial value unchanged.
- VCD declarations, widths, one-picosecond timescale, exact clock/counter/word
  transitions and final tick 9. HDL code coverage must show nine covered
  statements, three executions of the always block and three clock rises/falls.
  This is HDL code coverage, separate from e cover-group collection.

The report fingerprints simulator/interpreter/bridge binaries and owned
fixtures/implementation files. It retains the staged owned design, VCD and HDL
coverage JSON alongside the report. Reports contain no raw subprocess output.
Generated data stays under ignored local/; do not commit simulator output dumps.
The simulator version is observed from the executable, not inferred from an
image tag. These fingerprints are provenance, not build attestation.

## Reproduce

Build the current Dockerfile before using the compose service; an older local
image carrying the same tag may lack xezim or the revised bridge/Python runtime.

```sh
docker compose build
docker compose run --rm qualify-hdl
```

The service builds the current native interpreter and runs:

```sh
python3 scripts/qualify_hdl.py --native /opt/target/debug/e1647 \
  --report local/hdl-profile/report.json
```

For an existing Linux image containing the pinned xezim and its headers, build
the bridge and interpreter there, then run the same script. The observed
extended image was `ai-hw-engineer-extended:dev`, image ID
`sha256:2c2d7265c7d157ac99201232025af5faf975221693a7ebdc4aba80df8c031f54`.
It supplied xezim 0.11.3 and rustc 1.99.0. `/workspace` was a read-only mount
of this branch and `/artifacts` an ignored writable host directory:

```sh
cc -Wall -Wextra -Werror -shared -fPIC -I/opt/xezim/include \
  -o /artifacts/bridge.so c/xezim_bridge.c
cargo build --locked --target-dir /artifacts/target --bin e1647
E1647_XEZIM_BRIDGE=/artifacts/bridge.so python3 scripts/qualify_hdl.py \
  --native /artifacts/target/debug/e1647 --report /artifacts/qualification.json
```

A different simulator revision fails the profile instead of silently passing
under another identity. The runner requires Linux, Python 3 and absolute work
paths containing only letters, digits, `/_.-`. It creates a separate process
group per invocation and kills that group on its 15-second outer timeout.
Owned Rust subprocess tests and Python artifact-oracle tests are portable
mechanical checks. The separate check-hdl-profile CI job builds the pinned
simulator/current bridge image and runs real simulation. It retains the owned
profile report, staged inputs, VCD, HDL coverage and concrete image/tool identity
for seven days.

## Dockerfile and Compose verification, 2026-11-15

The current Dockerfile was built and its actual Compose services executed.
The local tag `e1647-rs-toolchain:hdl-profile` preserved the pre-existing
`:latest` image. The resulting image identity was
`sha256:a301937544b5ba48a37c2315187ff192f1c46be848fcb1ebdf4ef905bdee8d2d`.
The floating rust:1-slim base resolved to
`sha256:cc9edbcadf2c27013a642b1d1bb7504e26b1142d6b5ebe58f64cf2eb2bc128ae`.
This image reported Rust/Cargo 1.100.0, Python 3.13.5, GCC 14.2.0 and the
same pinned xezim 0.11.3 revision. Runtime library resolution succeeded.

```sh
docker build --tag e1647-rs-toolchain:hdl-profile .
```

An ignored local override selected only that image for the two services:

```yaml
services:
  qualify-hdl:
    image: e1647-rs-toolchain:hdl-profile
  verify:
    image: e1647-rs-toolchain:hdl-profile
```

Using that override, `docker compose -f compose.yml -f <override> run --rm
qualify-hdl` passed all eight cases, raw bridge assertions, exact VCD transitions
and HDL coverage assertions. The verify service passed `cargo fmt --all --
--check`, `cargo clippy --all-targets --all-features -- -D warnings` and
`cargo test --all-targets --all-features`: 513 passed, 0 failed, 0 ignored
across 27 binaries. Default-feature Windows checks separately passed all
504 tests. Generated reports/artifacts remained under ignored local/.

These observations qualify a concrete build; they do not promise identical
image bytes from future floating-base or upstream dependency resolutions.
Each newly built image must pass the profile. No simulator pin was changed.
Use the non-login shell in the Compose commands: a login shell can replace
the Dockerfile's PATH and hide otherwise installed tools.

## Bridge contract and limits

The UTF-8 line protocol reserves READY, OK, VAL, ERR and BYE as first tokens.
Other log lines are ignored. Token prefixes such as READYING are not replies;
malformed or out-of-order reserved replies fail. The untagged protocol cannot
distinguish a design printing an exact reserved reply from a bridge reply;
such output is outside this profile.

Startup and each complete command handshake have a 30-second deadline.
`E1647_XEZIM_TIMEOUT_MS` accepts 1..300000. RUN's OK and READY share a deadline;
ERR fails immediately. A line is limited to 16 KiB, skipped/response text per
exchange to 1 MiB, and the reader queue to eight lines. Transport/bridge errors
close and reap the owned child and preserve the failure for later operations.
The session does not manage descendant process trees. The qualification runner
adds process-group cleanup; ordinary standalone sessions do not.

For the pinned xezim backend, RUN acknowledges registration with OK. READY
comes from `cbReadWriteSynch` at the target tick, after active work and due
nonblocking assignments (NBA) have settled. e can read those values and
issue another SET from that writable checkpoint. `hdl_run(0)` settles due
work at the current tick. `hdl_run(1)` also advances one tick and exposes
updates due there. A read immediately after driving a clock, before any
step, can still observe the previous value. The e virtual scheduler advances
independently.

Signal paths contain 1..511 printable ASCII bytes without whitespace. The
bridge transports vpiIntVal, explicitly rejecting signals outside 1..32 bits,
X/Z reads and writes outside signed 32-bit range. A 32-bit high bit is read as
a signed integer; this is not a general packed-vector/MVL transport. Rejected
local paths/values send no command. The C bridge preserves the high/low halves
of unsigned 64-bit delays, while the current e CLI accepts nonnegative i64
delays. Actual large-delay behavior beyond the profile's 0/1 steps is not
qualified here, including the simulator's own maximum-time limits.

`E1647_XEZIM_WAVE=1` enables xezim's waveform instrumentation; 0/unset leaves
it off. The HDL design still selects its dump path. The profile uses owned
$dumpfile/$dumpvars actions and XEZIM_CODE_COVERAGE/XEZIM_COV_DB settings.
`hdl_finish` checks BYE and successful child exit, reports failure as E0408 and
reaps the process; the older Rust finish() cleanup entry point remains available
without returning errors. Static methods, on-action restrictions, general
HDL event ordering, VHDL/VHPI, arbitrary signal types and browser runtime support
remain outside this qualification.
