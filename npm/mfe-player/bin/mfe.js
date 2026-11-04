#!/usr/bin/env node
"use strict";

// Thin launcher: pick the optional platform package that npm already
// installed for this machine and hand it the arguments. No postinstall
// download, no proxy surprises.
const path = require("node:path");
const { spawnSync } = require("node:child_process");

function isMusl() {
  if (process.platform !== "linux") return false;
  try {
    return !process.report.getReport().header.glibcVersionRuntime;
  } catch {
    return true;
  }
}

function packages() {
  const { platform, arch } = process;
  if (platform === "linux") {
    const glibc = `mfe-player-linux-${arch}`;
    return isMusl() ? [`${glibc}-musl`, glibc] : [glibc];
  }
  if (platform === "darwin") return [`mfe-player-darwin-${arch}`];
  if (platform === "win32") return [`mfe-player-win32-${arch}`];
  return [];
}

function binary() {
  const file = process.platform === "win32" ? "mfe-player.exe" : "mfe-player";
  for (const name of packages()) {
    try {
      return require.resolve(`${name}/bin/${file}`);
    } catch {
      try {
        const root = require.resolve(`${name}/package.json`);
        return path.join(path.dirname(root), "bin", file);
      } catch {
        // try the next candidate
      }
    }
  }
  return null;
}

const bin = binary();
if (!bin) {
  console.error(
    `mfe-player: no prebuilt binary for ${process.platform}-${process.arch}`,
  );
  console.error("install it with: cargo install mfe-player");
  process.exit(1);
}

const result = spawnSync(bin, process.argv.slice(2), { stdio: "inherit" });
if (result.error) {
  console.error(`mfe-player: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status === null ? 1 : result.status);
