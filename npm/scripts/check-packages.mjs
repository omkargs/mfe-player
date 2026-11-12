// Refuse to publish a platform package without a real binary.
// Runs before `npm publish` in CI and locally (`node npm/scripts/check-packages.mjs`).
import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const main = JSON.parse(readFileSync(join(root, "mfe-player", "package.json"), "utf8"));
let failures = 0;

function fail(message) {
  console.error(`  x ${message}`);
  failures += 1;
}

for (const [name, version] of Object.entries(main.optionalDependencies ?? {})) {
  const dir = join(root, name);
  let manifest;
  try {
    manifest = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
  } catch {
    fail(`${name}: package.json missing`);
    continue;
  }
  if (manifest.version !== version) {
    fail(`${name}: version ${manifest.version} != launcher ${version}`);
  }
  const file = manifest.os?.includes("win32") ? "mfe-player.exe" : "mfe-player";
  const binary = join(dir, "bin", file);
  try {
    const stat = statSync(binary);
    if (stat.size < 1_000_000) fail(`${name}: ${file} is only ${stat.size} bytes`);
    if (!manifest.os?.includes("win32") && (stat.mode & 0o111) === 0) {
      fail(`${name}: ${file} is not executable`);
    }
  } catch {
    fail(`${name}: bin/${file} missing`);
  }
}

// The launcher must reference every directory that exists, and vice versa.
for (const entry of readdirSync(root, { withFileTypes: true })) {
  if (!entry.isDirectory() || !entry.name.startsWith("mfe-player-")) continue;
  if (entry.name === "mfe-player") continue;
  if (!(entry.name in (main.optionalDependencies ?? {}))) {
    fail(`${entry.name}: exists but is not an optional dependency`);
  }
}

if (failures > 0) {
  console.error(`\n${failures} packaging problem(s); refusing to publish.`);
  process.exit(1);
}
console.log(`all ${Object.keys(main.optionalDependencies ?? {}).length} platform packages look good`);
