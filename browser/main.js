import { Sandbox } from "./client.js";
const sandbox = new Sandbox();
const source = document.querySelector("#source");
const status = document.querySelector("#status");
const diagnostics = document.querySelector("#diagnostics");
const buttons = [...document.querySelectorAll("[data-operation]")];
const cancel = document.querySelector("#cancel");

function busy(value) { buttons.forEach((button) => { button.disabled = value; }); cancel.disabled = !value; }
for (const button of buttons) button.addEventListener("click", async () => {
  busy(true);
  status.textContent = "Working…";
  diagnostics.textContent = "None";
  document.querySelector("#output").textContent = "";
  document.querySelector("#coverage").textContent = "No collected groups";
  try {
    const result = await sandbox.request(source.value, { operation: button.dataset.operation, seed: document.querySelector("#seed").value });
    document.querySelector("#output").textContent = result.output;
    diagnostics.textContent = result.error ? `${result.error.code}: ${result.error.message}`
      : result.diagnostics.map((item) => `${item.start.line}:${item.start.column}: ${item.code}: ${item.message}`).join("\n") || "None";
    document.querySelector("#coverage").textContent = result.coverage.length ? JSON.stringify(result.coverage, null, 2) : "No collected groups";
    status.textContent = result.ok ? "Completed" : `Completed with diagnostics or check failures (${result.dutErrors} DUT errors)`;
  } catch (error) {
    status.textContent = error.code === "cancelled" ? "Cancelled" : "Request failed";
    diagnostics.textContent = `${error.code ?? "error"}: ${error.message}`;
  } finally { busy(false); }
});
cancel.addEventListener("click", () => sandbox.cancel());
