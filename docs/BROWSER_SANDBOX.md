# Experimental browser sandbox

The standalone default-feature WASM library exposes the existing declared
language subset through a versioned API. This does not extend the language
or claim IEEE conformance. The demo runs locally; it is not a hosted service.

Build with the Rust `wasm32-unknown-unknown` target installed:

```sh
node scripts/build_browser.mjs
python3 -m http.server 8000 --directory browser --bind 127.0.0.1
```

Open `http://127.0.0.1:8000`. Use `py` instead of `python3` on Windows.
The generated `browser/e1647.wasm` is ignored by Git. Serve over HTTP rather
than opening the HTML as a local file. A modern browser with module workers,
WebAssembly and BigInt is required. Chromium is the currently tested engine.

## JavaScript interface

```js
import { Sandbox } from './client.js';
const sandbox = new Sandbox();
const response = await sandbox.request(source, { operation: 'run', seed: '1' });
sandbox.cancel(); // Cancels an outstanding request, if any.
```

Operations are `parse`, `check` (the default), and `run`. Parse includes the
native declaration checks. Check adds composition/elaboration and conservative
type checking. Run executes only if those stages return no diagnostics.
Each request accepts one UTF-8 source, at most 256 KiB. Imports are explicitly
rejected, including imports nested in when members. There is no filesystem,
network, native simulator or user-supplied host callback exposed to e code.
`hdl_load` retains the WASM backend's no-simulator diagnostic.

The response has `version: 1`, `operation`, `seed`, `ok`, `output`,
`diagnostics`, `dutErrors`, `coverage` and `error`. Diagnostics contain
`code`, `message` and `start`/`end` positions with byte, line and column.
`ok` requires no API error, no diagnostics and zero failed DUT checks. A
failed DUT check is reported through `dutErrors` even when interpretation
finishes without a fatal diagnostic. `output` preserves the interpreter's
console text; the demo renders all returned content as text.

Coverage is a flat array of group summaries with type/name/unit owner,
samples, grade and basic items/buckets. Derived cross/transition structures
are not serialized. A group grade follows the native collector and can be
null. Coverage starts disabled until source config enables it. Seeds,
coverage counters/goals and signed 128-bit bucket bounds are **decimal
strings** to preserve precision; grades are JSON numbers or null. Positions,
API version and DUT counts are JSON numbers. The seed accepts the full u64
range as decimal text, including zero. Pass it as a string to avoid rounding
in the caller before the API receives it.

Pipeline failures resolve to `ok: false`, with diagnostics or `error`.
Client validation, load failures, WASM traps, deadlines and cancellation
reject with `SandboxError` containing `code` and `message`. One Sandbox
accepts one pending request; another request rejects with `busy`. It can be
reused after completion, failure or cancellation.

## Lifetime and limits

Each request creates a disposable module worker and a fresh WASM instance.
The host terminates it after 3000 ms by default; `timeoutMs` accepts integer
values from 5 through 60000. Cancellation terminates it immediately. The
deadline includes fetching and compiling the module. Browsers can delay
timers in background tabs, so this is not a hard real-time guarantee.
The build script sets 16 MiB initial linear memory and a 128 MiB maximum.
Allocation failure/traps discard the worker. The client checks that the
module has zero imports before instantiating it.

JSON results are limited to 1 MiB; excess results become `result_limit`
errors. Serialization and execution can allocate before that result limit
is checked; the linear-memory cap and worker deadline are the outer bounds.
The source string, browser module compilation and browser-owned allocations
sit outside the WASM memory cap. Native `sandbox::evaluate` calls do not
receive worker isolation/deadlines. This experimental API is not a security
audit or a resource guarantee for the entire browser process.

## ABI v1

The standalone module exports `memory` and these functions (u32 arguments
and results):

| Export | Contract |
| --- | --- |
| `e1647_api_version()` | Returns 1 |
| `e1647_source_limit()` | Returns 262144 |
| `e1647_alloc(length)` | Registers an owned input buffer; returns pointer or 0 |
| `e1647_free(pointer)` | Releases input; returns 1 if present, else 0 |
| `e1647_execute(pointer, length, operation, seed_lo, seed_hi)` | Returns an owned result handle or 0 at quota; operations 0/1/2 mean parse/check/run |
| `e1647_result_ptr(handle)` | JSON UTF-8 pointer, or 0 for unknown handle |
| `e1647_result_len(handle)` | JSON byte length, or 0 for unknown handle |
| `e1647_result_free(handle)` | Releases result; returns 1 if present, else 0 |

Write source bytes into the allocated buffer, then execute with its exact
registered length. Execute validates registered pointer/length and UTF-8
before reading the owned slice. Invalid buffers, UTF-8 and operation tags
return JSON API errors. There are eight outstanding input slots and eight
result slots per instance. Always release both kinds; re-read `memory.buffer`
after calls because memory can grow. Released pointers are invalid and may
be reused for later allocations. Never write outside an input buffer or
mutate memory while interpreting; this ABI is for a trusted host wrapper.
The wrapper discards the entire instance after a request.

ABI/response-limit error objects omit operation and seed, and retain version,
ok/output/diagnostics/coverage/dutErrors/error. No state is retained between
JavaScript requests. The shipped build uses default features, including the
bounded native constraint solver rather than the optional solver feature.

## Verification

```sh
npm ci --prefix browser --ignore-scripts
node browser/node_modules/playwright/cli.js install chromium
node scripts/build_browser.mjs
node scripts/test_browser.mjs
```

CI installs Chromium with its OS dependencies. Local installation can use
`PLAYWRIGHT_BROWSERS_PATH` to select a cache; `E1647_BROWSER_CHANNEL=chrome`
or `msedge` selects an installed channel. Tests start a loopback-only HTTP
server and close it and the browser afterward. They assert parse/check/run,
full-width seeds, spanned diagnostics, DUT checks, basic coverage, imports,
conditional binding type diagnostics,
retained type-expression diagnostics and execution,
interface implementation contracts and missing-method runtime staging,
composed extension contracts and static declaration/extend ordering,
tick/numeral disambiguation and qualified field/method execution,
method wrapper ordering/replacement, results, suspension and type diagnostics,
UTF-8/ABI quotas/releases, zero host imports, memory cap, synchronous guest
deadline, fetch deadline, cancellation/reuse, traps, client errors and demo
text rendering. The timeout/trap probes are independently authored tiny WASM
modules, separate from interpreter fixtures. Report and demo screenshot are
written under ignored `local/browser-sandbox/`. Native fixture tests use the
same independently authored sources. No Specman comparison is claimed.
