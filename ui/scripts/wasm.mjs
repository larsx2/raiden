import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ui = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const root = resolve(ui, "..");
const target = "wasm32-unknown-unknown";
const built = spawnSync(
  "cargo",
  [
    "rustc",
    "--quiet",
    "--package",
    "raiden-core",
    "--lib",
    "--release",
    "--target",
    target,
    "--crate-type",
    "cdylib",
  ],
  { cwd: root, stdio: "inherit" },
);
if (built.status !== 0) {
  process.exit(built.status ?? 1);
}
const folder = process.env["CARGO_TARGET_DIR"]
  ? resolve(root, process.env["CARGO_TARGET_DIR"])
  : join(root, "target");
mkdirSync(join(ui, "src", "generated"), { recursive: true });
copyFileSync(
  join(folder, target, "release", "raiden_core.wasm"),
  join(ui, "src", "generated", "search.wasm"),
);
