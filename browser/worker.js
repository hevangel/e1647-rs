function fail(code, message) { const error = new Error(message); error.code = code; throw error; }

self.onmessage = async ({ data }) => {
  try {
    const operations = { parse: 0, check: 1, run: 2 };
    if (!Object.hasOwn(operations, data.operation)) fail("invalid_operation", "Choose parse, check or run");
    if (!/^(0|[1-9][0-9]{0,19})$/.test(data.seed)) fail("invalid_seed", "Seed must be a decimal unsigned 64-bit integer");
    const seed = BigInt(data.seed);
    if (seed > 0xffffffffffffffffn) fail("invalid_seed", "Seed exceeds the unsigned 64-bit range");
    const response = await fetch(data.wasmUrl, { credentials: "omit" });
    if (!response.ok) fail("load_error", `Cannot load the WebAssembly module (${response.status})`);
    const module = await WebAssembly.compile(await response.arrayBuffer());
    if (WebAssembly.Module.imports(module).length !== 0) fail("host_imports", "This sandbox requires a module without host imports");
    const { exports: api } = await WebAssembly.instantiate(module, {});
    if (api.e1647_api_version() !== 1) fail("api_version", "Unsupported sandbox API version");
    const bytes = new TextEncoder().encode(data.source);
    if (bytes.length > api.e1647_source_limit()) fail("input_limit", "Source exceeds the browser input limit");
    const pointer = api.e1647_alloc(bytes.length) >>> 0;
    if (!pointer) fail("buffer_limit", "Cannot allocate a source buffer");
    let handle = 0;
    let result;
    try {
      new Uint8Array(api.memory.buffer, pointer, bytes.length).set(bytes);
      handle = api.e1647_execute(pointer, bytes.length, operations[data.operation], Number(seed & 0xffffffffn), Number(seed >> 32n)) >>> 0;
      if (!handle) fail("buffer_limit", "Cannot allocate a result buffer");
      const start = api.e1647_result_ptr(handle) >>> 0;
      const length = api.e1647_result_len(handle) >>> 0;
      if (!start || length > 1024 * 1024 || start + length > api.memory.buffer.byteLength) fail("invalid_result", "Invalid result buffer");
      // execute may grow memory: always take a fresh view afterwards.
      result = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(new Uint8Array(api.memory.buffer, start, length)));
    } finally {
      try { if (handle) api.e1647_result_free(handle); api.e1647_free(pointer); } catch { /* A trapped instance will be discarded. */ }
    }
    self.postMessage({ ok: true, response: result });
  } catch (error) {
    self.postMessage({ ok: false, error: { code: error.code ?? "wasm_error", message: error.message || "WebAssembly execution failed" } });
  }
};
