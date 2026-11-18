// Build only the standalone, default-feature WASM library. No host imports.
import { spawnSync } from "node:child_process";
import { copyFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const flags = [process.env.RUSTFLAGS ?? "", "-C link-arg=--initial-memory=16777216", "-C link-arg=--max-memory=134217728"].join(" ").trim();
const result = spawnSync("cargo", ["build", "--locked", "--release", "--lib", "--target", "wasm32-unknown-unknown"], {
  cwd: root, env: { ...process.env, RUSTFLAGS: flags }, stdio: "inherit",
});
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
const target = resolve(root, process.env.CARGO_TARGET_DIR ?? "target");
copyFileSync(resolve(target, "wasm32-unknown-unknown/release/e1647.wasm"), resolve(root, "browser/e1647.wasm"));
console.log("Built browser/e1647.wasm (128 MiB memory maximum)");
