# Differential case runner

The runner compares independently authored observable values for a selected
case corpus. It is not a conformance suite. The three initial candidates
cover instance-event ownership, list multiplicity and contextual values.
They pass natively; no new Specman reference run has been recorded for them.

Requires Python 3.10+ and a native e1647 binary. From the repository root:

```sh
cargo build --bin e1647
python3 scripts/qualify.py --native-only
```

On Windows use `py` instead of `python3`; the default binary path adds `.exe`.
For a separate Cargo target directory pass `--native /path/to/e1647`.
The report defaults to gitignored `local/qualification-report.json`.
CI runs the protocol tests and this explicit native-only mode.

A native-only success proves the native outputs match the owned expected
values. Its report uses `scope: native_expected_values_only`, sets the
reference status to `not_requested` and each comparison to `not_run`.
The CLI requires either `--native-only` or an explicit reference command;
missing reference access is never silently treated as a successful comparison.

## Authorized reference adapters

Provide an operator-managed wrapper for an authorized local simulator:

```sh
python3 scripts/qualify.py --reference-command '["/absolute/path/to/authorized-adapter"]' --timeout 90
```

Commands are JSON argument arrays, executed without a shell. For a launcher
with multiple arguments use, for example, `["python3", "/path/to/adapter.py"]`.
`--native-command` similarly accepts an argv array containing `{source}` and
`{seed}` placeholders, overriding `--native`. Credentials and license tokens
belong in the adapter's environment, never in these command arrays or reports.

The reference wrapper receives one JSON request on stdin:

```json
{
  "schema_version": 1,
  "case_id": "example-case",
  "source": "/absolute/staging/example-case.e",
  "source_sha256": "sha256-of-the-exact-source-bytes",
  "seed": 1,
  "work_dir": "/absolute/staging"
}
```

The wrapper must execute that exact source with the requested seed, confirm
successful compile/elaboration/execution and the absence of relevant tool/DUT
errors, and return **only** this JSON envelope on stdout:

```json
{
  "schema_version": 1,
  "case_id": "example-case",
  "source_sha256": "sha256-of-the-exact-source-bytes",
  "seed": 1,
  "status": "passed",
  "values": {"owned_result_key": "7"},
  "tool": {"name": "actual simulator name", "version": "actual version", "profile": "actual host/clock configuration"}
}
```

Use a nonzero exit or `status: failed` when compilation, elaboration, runtime
checks or extraction fail. Never map a vendor process's zero exit alone to
`passed`: some tools report DUT errors without a nonzero process exit. Extract
only the case's owned result values; do not copy simulator logs into the JSON.
Tool identity/version/profile are adapter-reported metadata, not independently
attested by the runner. The operator remains responsible for authorization,
correct simulator invocation, compatible host/clock setup and interpretation
of its result.

A fake process is used only by `qualification_runner_test.py` to verify this
protocol's failure handling. It is named `Owned protocol fixture` with version
`test-only`; those tests provide no simulator or language qualification.
There is no supplied or qualified Specman adapter yet. EDA Playground manual
runs follow [the repository skill](../skills/edaplayground/SKILL.md); this runner
does not automate its login, CAPTCHA, licensing or UI.

## Corpus and result contract

`cases.json` supplies unique IDs, repository-relative `.e` paths, unsigned
64-bit seeds, 2019 clause references and exact expected string values. The
source files are independently authored, self-contained candidates under
`tests/fixtures/qualification/`. Their code-segment delimiters work both in
the native CLI and a reference loader. No preprocessing changes their bytes.
Imported dependency staging is not implemented; keep these cases self-contained.

Native programs emit one line per owned result and then one completion line:

```text
E1647_RESULT owned_result_key=7
E1647_DONE example-case
```

Duplicate/malformed markers, results after completion, missing completion,
extra or missing result keys, and disagreement with the manifest fail. Native
and reference values must **both** match the independent expected values;
two backends agreeing on an unexpected answer do not pass.

Both backends receive the same staged source snapshot. Its hash is verified
after execution, and any mutation/deletion fails that backend. Reference input
is restored from the original snapshot before launch. Request identity fields
must match the adapter's response, including seed, source hash and case ID.

The JSON report records case IDs, source fingerprints, seeds, clause references,
expected/observed owned values, exit/status categories, command fingerprints,
native launcher fingerprint and adapter-reported tool metadata. It contains
no command arguments or raw subprocess stdout/stderr. Temporary staging lives
under gitignored `local/qualification-work/`; the runner requests cleanup after
each case.
Do not commit generated reports or proprietary output dumps.

Exit codes: 0 means all requested checks passed within the report's stated
scope; 1 means a backend/protocol/value check failed; 2 means configuration or
file access failed. Timeouts, failed exits, malformed envelopes and missing
executables fail rather than skip. `--timeout` applies to each backend process.
The wrapper owns simulator descendant cleanup: terminating the adapter does
not itself prove that every simulator subprocess or remote session stopped.
The output cap rejects more than 1 MiB after capture; it is not a streaming
memory limit. These process-boundary limits remain separate from language
or simulator qualification.

Record actual authorized runs and conclusions in
[docs/VERIFICATION.md](../docs/VERIFICATION.md), linking the exact source/case
revision and configured simulator. A report comparing one corpus cannot
support broader IEEE or commercial-tool compatibility claims.
