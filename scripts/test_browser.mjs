// Real browser execution, including worker lifetime and the standalone ABI.
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const { chromium } = createRequire(new URL("../browser/package.json", import.meta.url))("playwright");
const files = new Map(["index.html", "style.css", "main.js", "client.js", "worker.js", "e1647.wasm"].map(name => [`/${name}`, name]));
const mime = { html: "text/html", css: "text/css", js: "text/javascript", wasm: "application/wasm" };
// Tiny independently authored modules isolate synchronous WASM timeout/trap
// handling from the interpreter. They have no host imports.
const testModule = body => {
  const name = [...Buffer.from("e1647_api_version")];
  const exports = [1, name.length, ...name, 0, 0];
  const code = [1, body.length + 1, 0, ...body];
  return Buffer.from([0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 127,
    3, 2, 1, 0, 7, exports.length, ...exports, 10, code.length, ...code]);
};
const loopModule = testModule([3, 64, 12, 0, 11, 0, 11]);
const trapModule = testModule([0, 11]);
assert(WebAssembly.validate(loopModule)); assert(WebAssembly.validate(trapModule));
const server = createServer(async (req, res) => {
  if (req.url === "/slow.wasm") { req.on("close", () => res.destroy()); return; }
  if (req.url === "/loop.wasm" || req.url === "/trap.wasm") {
    res.writeHead(200, { "Content-Type": "application/wasm" }).end(req.url === "/loop.wasm" ? loopModule : trapModule); return;
  }
  const name = files.get(req.url === "/" ? "/index.html" : req.url);
  if (!name) { res.writeHead(404).end(); return; }
  try { res.writeHead(200, { "Content-Type": mime[name.split(".").at(-1)], "Cache-Control": "no-store" }); res.end(await readFile(resolve(root, "browser", name))); }
  catch { res.writeHead(500).end(); }
});
await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
let browser;
const checks = [];
try {
  browser = await chromium.launch({ headless: true, ...(process.env.E1647_BROWSER_CHANNEL ? { channel: process.env.E1647_BROWSER_CHANNEL } : {}) });
  const page = await browser.newPage();
  const errors = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  const fixture = async name => readFile(resolve(root, "tests/fixtures/wasm-api", `${name}.e`), "utf8");
  const request = async (source, options = {}) => page.evaluate(async ({ source, options }) => {
    const { Sandbox } = await import("/client.js");
    return new Sandbox().request(source, options);
  }, { source, options });
  const good = await fixture("run");
  for (const operation of ["parse", "check", "run"]) {
    const result = await request(good, { operation, seed: "18446744073709551615" });
    assert.equal(result.ok, true); assert.equal(result.version, 1); assert.equal(result.operation, operation);
    assert.equal(result.seed, "18446744073709551615");
    assert.equal(result.output, operation === "run" ? "10\n" : "");
    checks.push(`${operation} pipeline and full-width seed`);
  }
  const typed = await request(await fixture("type-error"), { operation: "run" });
  assert.equal(typed.ok, false); assert.equal(typed.output, "");
  assert.equal(typed.diagnostics[0].code, "E0220"); assert.equal(typed.diagnostics[0].start.line, 3);
  assert(typed.diagnostics[0].end.byte > typed.diagnostics[0].start.byte);
  const syntax = await request("extend sys {", { operation: "parse" });
  assert.equal(syntax.ok, false); assert(syntax.diagnostics.length > 0);
  const dut = await request(await fixture("dut-error"), { operation: "run" });
  assert.equal(dut.ok, false); assert.equal(dut.dutErrors, 1);
  const imported = await request(await fixture("import"), { operation: "run" });
  assert.equal(imported.error.code, "unsupported_import");
  checks.push("spanned syntax/type diagnostics, DUT failure and import rejection");
  const guardSource = await readFile(resolve(root, "tests/fixtures/typing-bindings/guards.e"), "utf8");
  const guards = await request(guardSource, { operation: "check" });
  assert.equal(guards.ok, false); assert.equal(guards.diagnostics.length, 10);
  const guardDiagnostics = guards.diagnostics.map(d => `${d.start.line}:${d.start.column}: error[${d.code}]: ${d.message}\n`).join("");
  assert.equal(guardDiagnostics, await readFile(resolve(root, "tests/fixtures/typing-bindings/guards.expected"), "utf8"));
  const guardRun = await request(await readFile(resolve(root, "tests/fixtures/typing-bindings/runtime.e"), "utf8"), { operation: "run" });
  assert.equal(guardRun.ok, true); assert.equal(guardRun.output, "guarded bindings passed");
  checks.push("conditional binding type diagnostics");
  for (const name of ["annotations", "values"]) {
    const directory = resolve(root, "tests/fixtures/typing-type-expressions");
    const result = await request(await readFile(resolve(directory, `${name}.e`), "utf8"), { operation: "check" });
    assert.equal(result.ok, false);
    assert.equal(result.diagnostics.map(d => `${d.start.line}:${d.start.column}: error[${d.code}]: ${d.message}\n`).join(""),
      await readFile(resolve(directory, `${name}.expected`), "utf8"));
  }
  const typedRun = await request(await readFile(resolve(root, "tests/fixtures/typing-type-expressions/runtime.e"), "utf8"), { operation: "run" });
  assert.equal(typedRun.ok, true); assert.equal(typedRun.output, "type expressions passed");
  checks.push("retained type expression diagnostics and execution");
  for (const name of ["mismatch", "composition", "widths"]) {
    const directory = resolve(root, "tests/fixtures/interface-signatures");
    const result = await request(await readFile(resolve(directory, `${name}.e`), "utf8"), { operation: "check" });
    assert.equal(result.ok, false);
    assert.equal(result.diagnostics.map(d => `${d.start.line}:${d.start.column}: error[${d.code}]: ${d.message}\n`).join(""),
      await readFile(resolve(directory, `${name}.expected`), "utf8"));
  }
  const interfaceDirectory = resolve(root, "tests/fixtures/interface-signatures");
  const interfaceRun = await request(await readFile(resolve(interfaceDirectory, "runtime.e"), "utf8"), { operation: "run" });
  assert.equal(interfaceRun.ok, true); assert.equal(interfaceRun.output, "interface signatures passed");
  const missingSource = await readFile(resolve(interfaceDirectory, "missing.e"), "utf8");
  assert.equal((await request(missingSource, { operation: "check" })).ok, true);
  assert.equal((await request(missingSource, { operation: "run" })).diagnostics[0].code, "E0414");
  checks.push("interface implementation contracts and missing-method runtime staging");
  const extensionDirectory = resolve(root, "tests/fixtures/method-extension-contracts");
  for (const name of ["mismatch", "conditional", "template", "template-reversed"]) {
    const source = await readFile(resolve(extensionDirectory, `${name}.e`), "utf8");
    if (name === "mismatch") assert.equal((await request(source, { operation: "parse" })).ok, true);
    const result = await request(source, { operation: "check" });
    assert.equal(result.ok, false);
    assert.equal(result.diagnostics.map(d => `${d.start.line}:${d.start.column}: error[${d.code}]: ${d.message}\n`).join(""),
      await readFile(resolve(extensionDirectory, `${name}.expected`), "utf8"));
  }
  const extensionRun = await request(await readFile(resolve(extensionDirectory, "runtime.e"), "utf8"), { operation: "run" });
  assert.equal(extensionRun.ok, true); assert.equal(extensionRun.output, "method extension contracts passed");
  checks.push("composed method extension contracts and static ordering");
  const tickDirectory = resolve(root, "tests/fixtures/tick-literals");
  const tickErrors = await request(await readFile(resolve(tickDirectory, "errors.e"), "utf8"), { operation: "check" });
  assert.equal(tickErrors.diagnostics.map(d => `${d.start.line}:${d.start.column}: error[${d.code}]: ${d.message}\n`).join(""),
    await readFile(resolve(tickDirectory, "errors.expected"), "utf8"));
  assert.equal((await request(await readFile(resolve(tickDirectory, "paths.e"), "utf8"), { operation: "parse" })).ok, true);
  const tickRun = await request(await readFile(resolve(tickDirectory, "runtime.e"), "utf8"), { operation: "run" });
  assert.equal(tickRun.ok, true); assert.equal(tickRun.output, "tick qualifiers passed");
  checks.push("tick/numeral disambiguation and qualified execution");
  const executionDirectory = resolve(root, "tests/fixtures/method-execution");
  for (const name of ["order", "arguments-results", "static", "tcm", "lifecycle", "sequence", "polymorphic"]) {
    const result = await request(await readFile(resolve(executionDirectory, `${name}.e`), "utf8"), { operation: "run" });
    assert.equal(result.ok, true, `${name}: ${JSON.stringify(result.diagnostics)}`);
    assert.equal(result.dutErrors, 0, name);
    assert.equal(result.output, await readFile(resolve(executionDirectory, `${name}.out`), "utf8"), name);
  }
  const resultErrors = await request(await readFile(resolve(executionDirectory, "result-errors.e"), "utf8"), { operation: "check" });
  assert.equal(resultErrors.ok, false);
  assert.equal(resultErrors.diagnostics.map(d => `${d.start.line}:${d.start.column}: error[${d.code}]: ${d.message}\n`).join(""),
    await readFile(resolve(executionDirectory, "result-errors.expected"), "utf8"));
  checks.push("method wrapper ordering, replacement, results and suspension");
  const covered = await request(await fixture("coverage"), { operation: "run" });
  assert.equal(covered.ok, true);
  const group = covered.coverage.find(group => group.name === "cg");
  assert.equal(group.samples, "2"); assert.equal(group.grade, 1);
  assert.deepEqual(group.items[0].buckets.map(bucket => bucket.hits), ["1", "1"]);
  checks.push("basic coverage summary");
  for (const name of ["coverage-graphs/cross-transition", "coverage-graphs/duplicate-labels",
    "coverage-graphs/empty-unbounded", "coverage-graphs/all-masked",
    "coverage-instances/per-unit-reporting", "wasm-api/derived-unbucketed"]) {
    const path = resolve(root, "tests/fixtures", name);
    const result = await request(await readFile(`${path}.e`, "utf8"), { operation: "run" });
    assert.equal(result.ok, true, name);
    assert.deepEqual(result.coverage, JSON.parse(await readFile(`${path}.browser.json`, "utf8")), name);
  }
  assert.deepEqual(group.derived, []);
  checks.push("derived coverage summaries, per-unit grades, hidden sources, filters, masks, duplicate labels and exact counters");
  for (const name of ["constant-names", "escaped-range-label"]) {
    const path = resolve(root, "tests/fixtures/coverage-graphs", name);
    const result = await request(await readFile(`${path}.e`, "utf8"), { operation: "run" });
    assert.equal(result.ok, true, name);
    assert.deepEqual(result.coverage, JSON.parse(await readFile(`${path}.browser.json`, "utf8")), name);
  }
  for (const name of ["constant-name-errors", "constant-name-duplicate"]) {
    const path = resolve(root, "tests/fixtures/coverage-graphs", name);
    const result = await request(await readFile(`${path}.e`, "utf8"), { operation: "check" });
    assert.equal(result.ok, false, name);
    assert.equal(result.diagnostics.map(d => `${d.start.line}:${d.start.column}: error[${d.code}]: ${d.message}\n`).join(""),
      await readFile(`${path}.expected`, "utf8"), name);
  }
  checks.push("constant coverage names, decoded range labels and exact name diagnostics");
  const namedDirectory = resolve(root, "tests/fixtures/coverage-graphs");
  const namedRun = await request(await readFile(resolve(namedDirectory, "named-constants.e"), "utf8"), { operation: "run" });
  assert.equal(namedRun.ok, true, JSON.stringify(namedRun.diagnostics));
  assert.deepEqual(namedRun.coverage, JSON.parse(await readFile(resolve(namedDirectory, "named-constants.browser.json"), "utf8")));
  const constantArithmetic = await request(await readFile(resolve(namedDirectory, "constant-arithmetic.e"), "utf8"), { operation: "run" });
  assert.equal(constantArithmetic.ok, true, JSON.stringify(constantArithmetic.diagnostics));
  assert.equal(constantArithmetic.coverage[0].grade, null);
  assert.deepEqual(constantArithmetic.coverage[0].items[0].buckets.map(bucket => bucket.goal), ["0", "0"]);
  for (const name of ["named-constant-errors", "constant-depth-limit", "constant-string-limit"]) {
    const result = await request(await readFile(resolve(namedDirectory, `${name}.e`), "utf8"), { operation: "check" });
    assert.equal(result.ok, false, name);
    const expectedSuffix = name === "constant-string-limit" ? "browser.expected" : "expected";
    assert.equal(result.diagnostics.map(d => `${d.start.line}:${d.start.column}: error[${d.code}]: ${d.message}\n`).join(""),
      await readFile(resolve(namedDirectory, `${name}.${expectedSuffix}`), "utf8"), name);
  }
  checks.push("named coverage constants, inherited/static initializers, precision and invalid dependencies");
  const representationRun = await request(await readFile(resolve(namedDirectory, "constant-representations.e"), "utf8"), { operation: "run" });
  assert.equal(representationRun.ok, true, JSON.stringify(representationRun.diagnostics));
  assert.equal(representationRun.output, await readFile(resolve(namedDirectory, "constant-representations.out"), "utf8"));
  assert.deepEqual(representationRun.coverage, JSON.parse(await readFile(resolve(namedDirectory, "constant-representations.browser.json"), "utf8")));
  const representationErrors = await request(await readFile(resolve(namedDirectory, "constant-representation-errors.e"), "utf8"), { operation: "check" });
  assert.equal(representationErrors.ok, false);
  assert.equal(representationErrors.diagnostics.map(d => `${d.start.line}:${d.start.column}: error[${d.code}]: ${d.message}\n`).join(""),
    await readFile(resolve(namedDirectory, "constant-representation-errors.expected"), "utf8"));
  checks.push("alias and sized constants with shared runtime precision and exact diagnostics");
  const failures = await page.evaluate(async () => {
    const { Sandbox } = await import("/client.js");
    const code = promise => promise.then(() => "unexpected_success", error => error.code);
    const sandbox = new Sandbox({ wasmUrl: "/slow.wasm", timeoutMs: 1000 });
    const pending = code(sandbox.request(""));
    const busy = await code(sandbox.request("")); sandbox.cancel();
    const cancelled = await pending;
    sandbox.wasmUrl = new URL("/e1647.wasm", location.href).href;
    const recovered = await sandbox.request("");
    return { busy, cancelled, recovered: recovered.ok, idle: !sandbox.busy,
      timeout: await code(new Sandbox({ wasmUrl: "/slow.wasm", timeoutMs: 50 }).request("")),
      cpuTimeout: await code(new Sandbox({ wasmUrl: "/loop.wasm", timeoutMs: 300 }).request("")),
      trap: await code(new Sandbox({ wasmUrl: "/trap.wasm" }).request("")),
      seed: await code(new Sandbox().request("", { seed: "18446744073709551616" })),
      operation: await code(new Sandbox().request("", { operation: "unknown" })),
      input: await code(new Sandbox().request("λ".repeat(131073))),
      load: await code(new Sandbox({ wasmUrl: "/missing.wasm" }).request("")) };
  });
  assert.deepEqual(failures, { busy: "busy", cancelled: "cancelled", recovered: true, idle: true,
    timeout: "timeout", cpuTimeout: "timeout", trap: "wasm_error", seed: "invalid_seed", operation: "invalid_operation", input: "input_limit", load: "load_error" });
  checks.push("deadline, cancellation, recovery, concurrency and client errors");
  const abi = await page.evaluate(async () => {
    const module = await WebAssembly.compile(await (await fetch("/e1647.wasm")).arrayBuffer());
    const { exports: api } = await WebAssembly.instantiate(module, {});
    const read = handle => JSON.parse(new TextDecoder().decode(new Uint8Array(api.memory.buffer, api.e1647_result_ptr(handle), api.e1647_result_len(handle))));
    const execute = (p, n, op = 0) => { const h = api.e1647_execute(p, n, op, 0xffffffff, 0xffffffff); const r = read(h); api.e1647_result_free(h); return r; };
    const invalid = execute(1, 5).error.code;
    const pointer = api.e1647_alloc(1); new Uint8Array(api.memory.buffer)[pointer] = 255;
    const utf8 = execute(pointer, 1).error.code;
    const length = execute(pointer, 0).error.code;
    new Uint8Array(api.memory.buffer)[pointer] = 32;
    const operation = execute(pointer, 1, 99).error.code;
    api.e1647_free(pointer);
    const freed = execute(pointer, 1).error.code;
    const inputs = Array.from({ length: 8 }, () => api.e1647_alloc(0));
    const inputLimit = api.e1647_alloc(0); const oversize = api.e1647_alloc(262145);
    const results = Array.from({ length: 8 }, () => api.e1647_execute(inputs[0], 0, 0, 1, 0));
    const resultLimit = api.e1647_execute(inputs[0], 0, 0, 1, 0);
    const validResults = results.every(h => h && read(h).ok);
    for (const h of results) api.e1647_result_free(h);
    const invalidResult = api.e1647_result_ptr(results[0]) | api.e1647_result_len(results[0]) | api.e1647_result_free(results[0]);
    inputs.forEach(p => api.e1647_free(p));
    const reused = api.e1647_alloc(0) !== 0;
    api.memory.grow(2048 - api.memory.buffer.byteLength / 65536);
    let capped = false; try { api.memory.grow(1); } catch (error) { capped = error instanceof RangeError; }
    return { imports: WebAssembly.Module.imports(module), version: api.e1647_api_version(),
      invalid, utf8, length, operation, freed, inputLimit, oversize, resultLimit, validResults, invalidResult, reused, capped };
  });
  assert.deepEqual(abi, { imports: [], version: 1, invalid: "invalid_buffer", utf8: "invalid_utf8", length: "invalid_buffer",
    operation: "invalid_operation", freed: "invalid_buffer", inputLimit: 0, oversize: 0, resultLimit: 0, validResults: true, invalidResult: 0, reused: true, capped: true });
  checks.push("zero host imports, ABI validation/quotas/releases and 128 MiB memory cap");
  await page.locator('#source').fill(good);
  await page.locator('[data-operation="run"]').click();
  await page.waitForFunction(() => document.querySelector('#status').textContent === "Completed");
  assert.equal(await page.locator('#output').textContent(), "10\n");
  await page.locator('#source').fill('extend sys { run() is also { out("<img src=x onerror=alert(1)>"); }; };');
  await page.locator('[data-operation="run"]').click();
  await page.waitForFunction(() => document.querySelector('#status').textContent === "Completed");
  assert.equal(await page.locator('#output img').count(), 0);
  assert.equal(await page.locator('#output').textContent(), '<img src=x onerror=alert(1)>');
  checks.push("demo controls and text-only rendering");
  assert.deepEqual(errors, []);
  const evidence = resolve(root, "local/browser-sandbox"); await mkdir(evidence, { recursive: true });
  await page.screenshot({ path: resolve(evidence, "demo.png"), fullPage: true });
  await writeFile(resolve(evidence, "report.json"), JSON.stringify({ browser: browser.version(), channel: process.env.E1647_BROWSER_CHANNEL ?? "chromium", checks }, null, 2) + "\n");
  console.log(`PASS: ${checks.length} browser check groups (${browser.version()})`);
} finally {
  await browser?.close();
  server.closeAllConnections(); await new Promise(resolve => server.close(resolve));
}
