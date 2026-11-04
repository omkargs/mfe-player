// Keep the launcher and every platform package on the same version.
// Usage: node npm/scripts/set-version.mjs 0.2.2
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const version = process.argv[2];
if (!version) {
  console.error("usage: set-version.mjs <version>");
  process.exit(1);
}

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const mainPath = join(root, "mfe-player", "package.json");
const main = JSON.parse(readFileSync(mainPath, "utf8"));
main.version = version;

for (const name of Object.keys(main.optionalDependencies ?? {})) {
  main.optionalDependencies[name] = version;
  const file = join(root, name, "package.json");
  const manifest = JSON.parse(readFileSync(file, "utf8"));
  manifest.version = version;
  writeFileSync(file, `${JSON.stringify(manifest, null, 2)}\n`);
}

writeFileSync(mainPath, `${JSON.stringify(main, null, 2)}\n`);
console.log(`npm packages set to ${version}`);
