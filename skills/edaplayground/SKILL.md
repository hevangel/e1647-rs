---
name: edaplayground
description: Run e (IEEE 1647 / Specman), SystemVerilog, or VHDL examples on edaplayground.com in the built-in browser and capture simulator logs as verification evidence. Use when a task needs a reference simulator run for e1647-rs, such as confirming e syntax, constraint behavior, or literal spellings against Cadence Specman.
---

# EDA Playground automation (e1647-rs)

EDA Playground is the lawful reference-simulator source for this project:
Cadence Specman runs there in the Cadence Xcelium flow. Every reference run
must be recorded in [docs/VERIFICATION.md](../../docs/VERIFICATION.md) —
what was run, which simulator, and the exact relevant log lines. This skill
is the site playbook: what we learned driving it with an automated browser.
For the browser mechanics themselves (bootstrap, tab recovery, DOM
snapshots, coordinate clicks) use the `browser-use:control-browser` skill.

## Credentials — from `.env` only

1. Read the account from the repo-root `.env` (gitignored; see
   `.env.example`): `EDA_PLAYGROUND_URL`, `EDA_PLAYGROUND_EMAIL`,
   `EDA_PLAYGROUND_PASSWORD`. Export them before browser work
   (`set -a; . .env; set +a`).
2. Never write the email or password into code, tests, examples, docs,
   skills, logs, screenshots, or commit messages. If a command would echo
   the password, redact it. Reference the account only as "the `.env`
   account".
3. If `.env` is missing or stale, stop and ask the owner; do not guess
   credentials.

## Login

1. Open `https://www.edaplayground.com`. A "Welcome to EDA Playground!"
   dialog with an inline login form means nobody is logged in; if the top
   area shows **Logout** instead, a session is already active — check
   whether it is the right account before reusing it.
2. Fill the **Username** and **Password** textboxes, tick the reCAPTCHA
   checkbox, click **Login**.
3. reCAPTCHA (observed 2026-10-25 failing twice, 2026-10-27 succeeding on
   the first attempt): the checkbox lives in a cross-origin iframe titled
   `reCAPTCHA` and `frameLocator` usually cannot attach to it. Click by
   coordinates: read `iframe[title="reCAPTCHA"]` with
   `getBoundingClientRect()` via `evaluate`, then click at
   (left + ~34px, vertical center). Wait ~5 s for the token and **verify
   the green check with a clipped screenshot before submitting** — a bad
   submit consumes an attempt. If the submit lands on
   `/loginpage?error=captcha` twice, stop and report; never solve image
   challenges or bypass the check. A manually pre-authenticated browser
   session is the fallback.
4. Success looks like: the URL stays on `/` (no error redirect) and a
   **Logout** link appears in the account menu.

## Setting up an e test

1. Language dropdown (hidden native `<select id="testbenchLanguage">`;
   read/confirm via `evaluate`, option text is exact): **Specman e +
   SV/Verilog**. There is no plain `e` entry.
2. Simulator (`<select id="simulator">`): **Cadence Xcelium …** (Specman
   runs inside the Xcelium `xrun` flow; the version select is
   `specmanVersion`). `methodologyLibrary` stays **None** for plain e.
   These selects are invisible — drive them with `evaluate` + change
   events, and confirm by reading `selectedOptions[0].text`.
3. Editors are CodeMirror instances under `.CodeMirror`: index 0 is the
   testbench (tab `testbench.e`), index 1 is the Design pane
   (`design.sv`). Read/write with `el.CodeMirror.getValue()/setValue()`
   from `evaluate`.
4. Wrap the e source in `<' ... '>` code-block delimiters, following the
   site convention, so the file compiles as a top-level e source. A
   minimal program extends the predefined `sys` unit and prints from
   `run()`.

## Running and reading results

1. Click **Run** (`#runButton`). If a click does not land, a hidden
   Bootstrap modal is overlaying the page — check
   `document.querySelectorAll('.modal')` for one with `in`/`show`, or
   probe with `document.elementFromPoint` over the button.
2. **Pure-e runs need an SV top** (learned 2026-10-27): with an empty
   Design pane the run fails at elaboration —
   `xmvlog: *W,NOTOPL: no top-level unit found` then
   `xmelab: *F,NOTOP: A top level design unit was not precalculated …`.
   Fix: put a minimal module in the Design editor, e.g.
   `` `timescale 1ns/1ns\nmodule design; endmodule ``. The e code does not
   need it; the SV elaborator does.
3. **Account validation gate:** commercial simulators require a validated
   account. An unvalidated account pops `#validatedAccessRequiredModal`
   ("User Validation Required — Your account is not validated") and the
   run silently does not start; the modal's `#resend` button re-sends the
   confirmation email, but clicking the emailed link is owner-only. A
   validated account runs with no modal.
4. Poll `#results` innerText (every ~5 s, up to ~75 s) until it contains
   `Exiting on` — a plain-e run takes ~15–20 s including Specman compile.
5. Success shape: `Generating the test with IntelliGen using seed N...`,
   the test's `out()`/`print` lines, `Checking is complete - 0 DUT
   errors, 0 DUT warnings`, and no `Exit code expected: 0, received:`
   mismatch line. Read the log as text, never as a screenshot.
6. Record in docs/VERIFICATION.md: the code (or the playground share URL),
   the simulator and Specman versions, the trimmed tool log, and what was
   concluded. Quote only tool output — never account emails or passwords.

## Saving and ownership

- **Save** (`#saveButton`) only works for the playground's owner.
- Viewing someone else's playground shows a **Copy** link instead (an `<a>`
  whose text is exactly `Copy`, `href="javascript:void(0)"`). Clicking it
  forks the playground to the logged-in account and redirects to a new
  `/x/xxxx` URL — record that URL as the reproducible one.
- Free accounts have limited save slots: keep one well-named playground per
  experiment series and prefer copying-and-updating over accumulating.

## Etiquette

- Runs share a public service: keep tests small, avoid long loops, and
  batch related experiments into one playground rather than spamming runs.
- If the site rate-limits or the simulator queue is busy, wait or report —
  never retry in a tight loop.
- Log out only when asked; leaving the session open in the automation
  browser is fine within a working session.

## Pitfall summary (each one cost us a run or a login attempt)

- reCAPTCHA rejects blind submits — verify the green check first, and stop
  after two captcha errors.
- Empty Design pane → `xmelab: *F,NOTOP`; always ship a minimal `module
  design` top with pure-e testbenches.
- Unvalidated account → Run does nothing visible except
  `#validatedAccessRequiredModal`; there is no programmatic workaround.
- The language/simulator dropdowns and editors are hidden native elements
  (`#testbenchLanguage`, `#simulator`, `.CodeMirror[i]`) — snapshot alone
  will not show their values; read them with `evaluate`.
- Save vs Copy is ownership-based; a saved URL under one account cannot be
  updated from another.
- Pasting code through a tool-call transport can turn `\n` escapes inside
  e strings into real newlines (Specman then fails with
  `Unterminated '"'`). Build the editor content with a placeholder split
  on `String.fromCharCode(92) + "n"` and verify a sample line contains a
  literal backslash before running.
