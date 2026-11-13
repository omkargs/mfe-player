#!/usr/bin/env node
"use strict";

// Thin launcher: pick the optional platform package npm already installed for
// this machine and hand it the arguments. If npm skipped the optional package
// (common on Alpine, ARM, or with `--no-optional`), fall back to downloading
// the matching GitHub release into the user cache, verify its SHA-256 and run
// that instead.
//
// Before the first real launch on Unix it runs the bundled installer in
// --setup mode: runtime tools (mpv, yt-dlp, python), shell completions and a
// doctor check. MFE_SKIP_SETUP=1 disables it; MFE_FORCE_SETUP=1 re-runs it.
const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

const RELEASE_BASE =
  process.env.MFE_RELEASE_BASE ||
  "https://github.com/omkargs/mfe-player/releases/latest/download";

function cacheDir() {
  if (process.platform === "win32") {
    const base = process.env.LOCALAPPDATA || path.join(os.homedir(), "AppData", "Local");
    return path.join(base, "mfe-player");
  }
  const base = process.env.XDG_CACHE_HOME || path.join(os.homedir(), ".cache");
  return path.join(base, "mfe-player");
}

function setupMarker() {
  return path.join(cacheDir(), "npm-setup.done");
}

function isAlpine() {
  try {
    return fs.existsSync("/etc/alpine-release");
  } catch {
    return false;
  }
}

function isMusl() {
  if (process.platform !== "linux") return false;
  if (isAlpine()) return true;
  // process.report is the reliable source on glibc systems. Fall back to
  // probing ldd for musl when the report is unavailable.
  try {
    return !process.report.getReport().header.glibcVersionRuntime;
  } catch {
    const probe = spawnSync("sh", ["-c", "ldd --version 2>&1 | head -1"], {
      encoding: "utf8",
    });
    return /musl/i.test(probe.stdout || "");
  }
}

function isRosetta() {
  if (process.platform !== "darwin" || process.arch !== "x64") return false;
  const probe = spawnSync("sysctl", ["-n", "sysctl.proc_translated"], {
    encoding: "utf8",
  });
  return (probe.stdout || "").trim() === "1";
}

// `arch` may differ from process.arch under Rosetta: an x64 Node on Apple
// Silicon is better served by the arm64 binary.
function effectiveArch() {
  if (isRosetta()) return "arm64";
  return process.arch;
}

function packages() {
  const { platform } = process;
  const arch = effectiveArch();
  if (platform === "linux") {
    const glibc = `mfe-player-linux-${arch}`;
    return isMusl() ? [`${glibc}-musl`, glibc] : [glibc];
  }
  if (platform === "darwin") return [`mfe-player-darwin-${arch}`];
  if (platform === "win32") return [`mfe-player-win32-${arch}`];
  return [];
}

function targetTriple() {
  const arch = effectiveArch();
  if (process.platform === "linux") {
    const suffix = isMusl() ? "unknown-linux-musl" : "unknown-linux-gnu";
    return `${arch === "x64" ? "x86_64" : arch}-${suffix}`;
  }
  if (process.platform === "darwin") {
    return `${arch === "x64" ? "x86_64" : "aarch64"}-apple-darwin`;
  }
  if (process.platform === "win32") {
    return `${arch === "x64" ? "x86_64" : "aarch64"}-pc-windows-msvc`;
  }
  return null;
}

function binaryName() {
  return process.platform === "win32" ? "mfe-player.exe" : "mfe-player";
}

function installedBinary() {
  const file = binaryName();
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

function run(command, args, options = {}) {
  return spawnSync(command, args, { stdio: "inherit", ...options });
}

function have(command) {
  const probe = spawnSync(
    process.platform === "win32" ? "where" : "sh",
    process.platform === "win32" ? [command] : ["-c", `command -v ${command} >/dev/null 2>&1`],
    { stdio: "ignore" },
  );
  return probe.status === 0;
}

function loadLatest() {
  const file = path.join(cacheDir(), "latest.json");
  if (!fs.existsSync(file)) return null;
  try {
    return JSON.parse(fs.readFileSync(file, "utf8"));
  } catch {
    return null;
  }
}

function sha256(file) {
  return crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");
}

// curl handles redirects, proxies (HTTPS_PROXY/NO_PROXY) and progress better
// than re-implementing them. Windows 10+ and every supported Unix ship curl.
function download(url, destination) {
  fs.mkdirSync(path.dirname(destination), { recursive: true });
  const result = run("curl", [
    "-fL",
    "--retry",
    "2",
    "--connect-timeout",
    "15",
    "-o",
    destination,
    url,
  ]);
  return result.status === 0 && fs.existsSync(destination);
}

function extract(archive, destination) {
  fs.mkdirSync(destination, { recursive: true });
  if (archive.endsWith(".zip")) {
    if (process.platform === "win32") {
      return (
        run("powershell", [
          "-NoProfile",
          "-Command",
          `Expand-Archive -Force -LiteralPath '${archive}' -DestinationPath '${destination}'`,
        ]).status === 0
      );
    }
    return run("unzip", ["-oq", archive, "-d", destination]).status === 0;
  }
  return run("tar", ["-xzf", archive, "-C", destination]).status === 0;
}

function downloadedBinary() {
  const triple = targetTriple();
  if (!triple) return null;
  const dir = path.join(cacheDir(), "bin", triple);
  const binary = path.join(dir, binaryName());
  if (fs.existsSync(binary)) return binary;

  const ext = process.platform === "win32" ? "zip" : "tar.gz";
  const name = `mfe-player-${triple}.${ext}`;
  const archive = path.join(cacheDir(), "downloads", name);
  let meta = loadLatest()?.targets?.[triple];
  if (!meta) {
    // Fetch the manifest once so the fallback download can be verified.
    const manifest = path.join(cacheDir(), "latest.json");
    if (download(`${RELEASE_BASE}/latest.json`, manifest)) {
      meta = loadLatest()?.targets?.[triple];
    }
  }

  console.error(`mfe-player: no bundled binary for ${process.platform}-${effectiveArch()}`);
  console.error(`mfe-player: downloading ${triple} from the latest release…`);

  if (!download(meta?.url || `${RELEASE_BASE}/${name}`, archive)) {
    return null;
  }
  if (meta?.sha256) {
    const got = sha256(archive);
    if (got !== meta.sha256) {
      console.error(`mfe-player: checksum mismatch for ${name}`);
      console.error(`  expected ${meta.sha256}`);
      console.error(`  got      ${got}`);
      return null;
    }
  }
  if (!extract(archive, dir)) return null;
  if (process.platform !== "win32") {
    try {
      fs.chmodSync(binary, 0o755);
    } catch {
      // best effort
    }
  }
  return fs.existsSync(binary) ? binary : null;
}

function resolveBinary() {
  if (process.env.MFE_BINARY) return process.env.MFE_BINARY;
  return installedBinary() || downloadedBinary();
}

function missingDependencies() {
  const missing = [];
  if (!have("mpv")) missing.push("mpv");
  if (!have("yt-dlp")) missing.push("yt-dlp");
  if (!have("python3") && !have("python")) missing.push("python3");
  return missing;
}

function guidance() {
  const arch = effectiveArch();
  if (process.platform === "linux") {
    console.error("  Debian/Ubuntu: sudo apt install mpv yt-dlp python3 ffmpeg");
    console.error("  Arch:          sudo pacman -S mpv yt-dlp python ffmpeg");
    console.error("  Fedora:        sudo dnf install mpv yt-dlp python3 ffmpeg");
    if (arch !== "x64" && arch !== "arm64") {
      console.error(`  no prebuilt Linux binary for ${arch}; build with: cargo install mfe-player`);
    }
  } else if (process.platform === "darwin") {
    console.error("  brew install mpv yt-dlp python ffmpeg");
  } else if (process.platform === "win32") {
    console.error("  winget install mpv yt-dlp Python.Python.3.12 Gyan.FFmpeg");
    console.error("  or run the PowerShell installer: npm/mfe-player/install.ps1");
  }
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
  "taste",
  "discord",
  "motion",
  "update",
  "cache",
  "completions",
  "help",
  "version",
  "play",
  "pause",
  "next",
  "prev",
  "mute",
  "seek",
  "volume",
  "shuffle",
  "repeat",
]);

// The same block-art banner install.sh opens with. Only shown on the first
// interactive launch of a playback command, so waybar/polybar JSON stays clean.
const LOGO_ART = `  ███╗   ███╗███████╗███████╗
  ████╗ ████║██╔════╝██╔════╝
  ██╔████╔██║█████╗  █████╗
  ██║╚██╔╝██║██╔══╝  ██╔══╝
  ██║ ╚═╝ ██║██║     ███████╗
  ╚═╝     ╚═╝╚═╝     ╚══════╝
  ██████╗ ██╗      █████╗ ██╗   ██╗███████╗██████╗
  ██╔══██╗██║     ██╔══██╗╚██╗ ██╔╝██╔════╝██╔══██╗
  ██████╔╝██║     ██████╔╝ ╚████╔╝ █████╗  ██████╔╝
  ██╔═══╝ ██║     ██╔══██║  ╚██╔╝  ██╔══╝  ██╔══██╗
  ██║     ███████╗██║  ██║   ██║   ███████╗██║  ██║
  ╚═╝     ╚══════╝╚═╝  ╚═╝   ╚═╝   ╚══════╝╚═╝  ╚═╝`;

function showSplash(args) {
  if (!process.stdout.isTTY) return false;
  const first = args[0] || "";
  if (first.startsWith("-") || NON_PLAY.has(first)) return false;
  const marker = setupMarker();
  const firstRun = !fs.existsSync(marker);
  if (!firstRun) return false;
  console.log(LOGO_ART);
  console.log("  a terminal music player · no account · no subscription\n");
  if (process.platform === "win32") {
    console.error("  ! Windows is EXPERIMENTAL - some features may not work yet.");
    console.error("    audio, media keys and desktop integrations are best-effort.");
    console.error("    the Docker build is the stable path: docker compose run --rm mfe\n");
  }
  return true;
}

function shouldSetup(args) {
  if (process.env.MFE_SKIP_SETUP) return false;
  // The installer is a bash script; Windows uses install.ps1 separately.
  if (process.platform === "win32") return false;
  const first = args[0] || "";
  if (first.startsWith("-")) return false;
  if (NON_PLAY.has(first)) return false;
  return true;
}

function setup(bin) {
  const missing = missingDependencies();
  const marker = setupMarker();
  const firstRun = !fs.existsSync(marker);
  // MFE_FORCE_SETUP=1 re-runs the setup even when everything is present.
  if (missing.length === 0 && !firstRun && !process.env.MFE_FORCE_SETUP) return;

  const script = path.join(__dirname, "..", "install.sh");
  if (!fs.existsSync(script)) {
    if (missing.length > 0) {
      console.error(`mfe-player: missing ${missing.join(", ")}`);
      guidance();
    }
    return;
  }

  const reason =
    missing.length > 0
      ? `setting up ${missing.join(", ")}`
      : "running first-time setup (completions + doctor)";
  console.error(`mfe-player: ${reason} before launch (MFE_SKIP_SETUP=1 skips this)`);

  const args = [script, "--setup"];
  // Test hook: exercise the setup path without touching the machine.
  if (process.env.MFE_SETUP_DRY_RUN) args.push("--dry-run");
  const result = spawnSync("bash", args, {
    stdio: "inherit",
    env: { ...process.env, MFE_BIN: bin },
  });

  if (result.error) {
    console.error(`mfe-player: could not run the installer: ${result.error.message}`);
  } else if (result.status !== 0) {
    console.error("mfe-player: setup did not finish; run `mfe doctor` for details.");
  } else if (!process.env.MFE_SETUP_DRY_RUN) {
    fs.mkdirSync(cacheDir(), { recursive: true });
    fs.writeFileSync(marker, `${new Date().toISOString()}\n`);
  }
}

const argv = process.argv.slice(2);
showSplash(argv);

const bin = resolveBinary();
if (!bin) {
  console.error(
    `mfe-player: no prebuilt binary for ${process.platform}-${effectiveArch()}`,
  );
  guidance();
  console.error("  build from source: cargo install mfe-player");
  process.exit(1);
}

if (shouldSetup(argv)) {
  setup(bin);
}

const result = spawnSync(bin, argv, { stdio: "inherit" });
if (result.error) {
  console.error(`mfe-player: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status === null ? 1 : result.status);
