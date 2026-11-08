#!/usr/bin/env node
"use strict";

// Thin launcher: pick the optional platform package that npm already
// installed for this machine and hand it the arguments. No postinstall
// download, no proxy surprises.
//
// Before the first real launch it also makes sure the runtime tools exist
// (mpv, yt-dlp, python3). If any are missing it runs the bundled installer
// with --deps-only, visibly, and then continues. MFE_SKIP_SETUP=1 disables it.
const fs = require("node:fs");
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

// Subcommands that are not "start playing music". They must never trigger a
// dependency install (mfe --version, mfe doctor, shell completions, ...).
const NON_PLAY = new Set([
  "status",
  "current",
  "share",
  "toggle",
  "stop",
  "daemon",
  "doctor",
  "theme",
  "update",
  "cache",
  "completions",
  "help",
  "version",
]);

function shouldSetup(args) {
  if (process.env.MFE_SKIP_SETUP) return false;
  // The installer is a bash script; Windows uses the binary directly.
  if (process.platform === "win32") return false;
  const first = args[0] || "";
  if (first.startsWith("-")) return false;
  if (NON_PLAY.has(first)) return false;
  return true;
}

function has(command) {
  const probe = spawnSync("sh", ["-c", `command -v ${command} >/dev/null 2>&1`], {
    stdio: "ignore",
  });
  return probe.status === 0;
}

function missingDependencies() {
  const missing = [];
  if (!has("mpv")) missing.push("mpv");
  if (!has("yt-dlp")) missing.push("yt-dlp");
  if (!has("python3") && !has("python")) missing.push("python3");
  return missing;
}

function setup() {
  const missing = missingDependencies();
  // MFE_FORCE_SETUP=1 re-runs the setup even when everything is present.
  if (missing.length === 0 && !process.env.MFE_FORCE_SETUP) return;

  const script = path.join(__dirname, "..", "install.sh");
  if (!fs.existsSync(script)) {
    console.error(
      `mfe-player: missing ${missing.join(", ")}; install them with your package manager.`,
    );
    return;
  }

  const reason =
    missing.length > 0
      ? `setting up ${missing.join(", ")}`
      : "re-running dependency setup";
  console.error(
    `mfe-player: ${reason} before launch (MFE_SKIP_SETUP=1 skips this)`,
  );
  const args = [script, "--deps-only"];
  // Test hook: lets the launcher exercise the setup path without touching
  // the machine. The installer itself prints the dry-run banner.
  if (process.env.MFE_SETUP_DRY_RUN) args.push("--dry-run");
  const result = spawnSync("bash", args, { stdio: "inherit" });
  if (result.error) {
    console.error(`mfe-player: could not run the installer: ${result.error.message}`);
  } else if (result.status !== 0) {
    console.error("mfe-player: setup did not finish; run `mfe doctor` for details.");
  }
}

const bin = binary();
if (!bin) {
  console.error(
    `mfe-player: no prebuilt binary for ${process.platform}-${process.arch}`,
  );
  console.error("install it with: cargo install mfe-player");
  process.exit(1);
}

if (shouldSetup(process.argv.slice(2))) {
  setup();
}

const result = spawnSync(bin, process.argv.slice(2), { stdio: "inherit" });
if (result.error) {
  console.error(`mfe-player: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status === null ? 1 : result.status);
