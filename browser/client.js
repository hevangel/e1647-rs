// Each request owns a fresh worker and WebAssembly instance. This host-side
// timer can stop code even while the guest is executing synchronously.
export class SandboxError extends Error {
  constructor(code, message) { super(message); this.name = "SandboxError"; this.code = code; }
}

export class Sandbox {
  constructor({ wasmUrl = new URL("./e1647.wasm", import.meta.url), timeoutMs = 3000 } = {}) {
    if (!Number.isInteger(timeoutMs) || timeoutMs < 5 || timeoutMs > 60000) throw new RangeError("Invalid worker deadline");
    this.wasmUrl = String(wasmUrl);
    this.timeoutMs = timeoutMs;
    this.pending = null;
  }
  get busy() { return this.pending !== null; }
  request(source, { operation = "check", seed = "1" } = {}) {
    if (this.pending) return Promise.reject(new SandboxError("busy", "A request is already running"));
    if (typeof source !== "string") return Promise.reject(new SandboxError("invalid_source", "Source must be text"));
    return new Promise((resolve, reject) => {
      const worker = new Worker(new URL("./worker.js", import.meta.url), { type: "module" });
      const finish = (error, response) => {
        if (this.pending?.worker !== worker) return;
        clearTimeout(this.pending.timer);
        worker.terminate();
        this.pending = null;
        if (error) reject(error); else resolve(response);
      };
      const timer = setTimeout(() => finish(new SandboxError("timeout", "Execution exceeded the worker deadline")), this.timeoutMs);
      this.pending = { worker, timer, finish };
      worker.onmessage = ({ data }) => data.ok
        ? finish(null, data.response)
        : finish(new SandboxError(data.error.code, data.error.message));
      worker.onerror = (event) => {
        event.preventDefault();
        finish(new SandboxError("worker_error", event.message || "The worker failed"));
      };
      worker.postMessage({ source, operation, seed: String(seed), wasmUrl: this.wasmUrl });
    });
  }
  cancel() {
    if (this.pending) this.pending.finish(new SandboxError("cancelled", "Execution cancelled"));
  }
}
