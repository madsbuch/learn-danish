// Generate all native icons directly from the vector artwork. No browser needed.
// Run after bun install: node scripts/make-icon.mjs
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const tauri = resolve(root, "node_modules/.bin/tauri");
execFileSync(tauri, ["icon", "scripts/icons.json"], { cwd: root, stdio: "inherit" });
const scratch = mkdtempSync(join(tmpdir(), "dansk-icons-"));
try {
  execFileSync(tauri, ["icon", "public/icon.svg", "--output", scratch, "--png", "1024"], {
    cwd: root, stdio: "inherit",
  });
  copyFileSync(join(scratch, "1024x1024.png"), resolve(root, "src-tauri/icons/source.png"));
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
