# Changelog

## 0.3.0 — 2026-11-12

Cross-platform, taste-aware and a lot prettier.

### Added

- **Taste autoplay.** When the queue ends with repeat off, a local model keeps
  the same vibe going using tags, artists, plays, skips and completion rates.
  Two profiles (30-day long-term, 90-minute session), MMR diversity and a
  guarded out-of-profile probe. `mfe taste [status|explain|reset|like|dislike]`
  shows and edits what it learned. Everything stays in `taste.json`.
- **Discord Rich Presence.** Opt-in local IPC presence with title, artist,
  album, progress, play/pause art and project buttons. `mfe discord
  on|off|status|client <id>|asset <key>`. No token, server or history leaves
  the machine; paused tracks are hidden by default.
- **Windows ARM64 package** and a PowerShell installer (`install.ps1`) that
  verifies checksums, updates the user PATH, offers winget dependency installs
  and registers completions.
- **`checksums.txt` + `latest.json`** release manifests, plus a verified
  download fallback in the npm launcher for missing optional packages.
- **Two themes:** `rose-pine` and `kanagawa`.

### Changed

- Platform layer (`src/platform/`) with MPRIS on Linux and inert media stubs
  elsewhere; `mpris-server` and `libc` are now platform-scoped dependencies.
- Windows: named-pipe mpv probing, `CREATE_NO_WINDOW` spawning, Python plugin
  launch and platform-aware doctor output.
- Daemon: graceful stop via a marker file and a heartbeat that makes stale pid
  files and PID reuse harmless.
- npm launcher: Alpine/musl detection, Rosetta preference, proxy-aware
  downloads and Node >= 18.
- UI polish: drifting gradients, marquee titles, pulsing player and cover
  frames, animated playing markers, autoplay badges, spectrum peak caps and
  live footer chips (taste/radio/discord/sleep).
- CLI output now exits quietly on a closed pipe instead of panicking.
- Version bumped to 0.3.0 across Cargo, PyPI and all npm packages.

### Fixed

- CLI config commands (`mfe discord`, `mfe theme`) now create the config
  directory on a fresh machine instead of silently doing nothing.
- Alt+V (full-screen now-playing) was unreachable and is now wired up.
- Atomic state/config/taste writes on Windows via `replace_file`.

## Unreleased

### Added

- **CLI integration:** `mfe status --json` for waybar/polybar/tmux, `mfe
  current`, and bash/zsh/fish completions.
- **Listening history & stats:** every play is recorded in a History view (`7`);
  `Ctrl+G` shows plays, listening time and top artists/tracks.
- **Truecolor album art:** covers render as half-blocks in the inspector in any
  modern terminal, cached locally.
- **Local library:** scan `library_paths`, read tags and embedded covers with
  `lofty`, browse with `8`, filter with `/`, rescan with `R`.
- **Share now playing:** `Y` or `mfe share` copies a one-line track description
  to the clipboard via wl-copy/xclip/xsel/pbcopy.
- **ListenBrainz scrobbling:** optional token in the config submits now-playing
  and listens from a background thread.
- **Smart fade:** `Alt+F` ramps volume down at the end of a track and back up
  at the start of the next one (0/2/4/6 s, persisted in config).
- **ReplayGain:** `Alt+R` cycles off/track/album for tagged files, pushed to
  mpv on startup and whenever it changes.
- **Editable EQ:** `Ctrl+E` opens a 10-band equaliser (31 Hz–16 kHz) with
  ±0.5 dB nudging; the curve is persisted and rendered as an mpv filter chain.
- **Internet Archive:** a new metadata + streaming bundle searches archive.org
  for free music, live sets and public-domain recordings and resolves them to
  direct mp3/flac URLs.
- **Radio Browser:** another metadata + streaming bundle queries
  radio-browser.info for live stations worldwide and pipes them straight into
  mpv (mpv plays ICY/SHOUTcast natively).
- **Transparent background:** `Alt+W` (or `transparent = true`) leaves the
  base background unpainted so the terminal's own opacity and blur show the
  wallpaper through. Panels stay solid for readability.
- **Wallpaper palette:** `theme = "wallpaper"`, `mfe theme adopt` or the
  `wallpaper` entry in the theme cycle derives a dark palette from the current
  wallpaper (swww, hyprpaper, GNOME, Plasma, macOS, Windows). Supports an
  explicit `wallpaper = "/path"` and `MFE_WALLPAPER`.
- **Aesthetic pass:** rounded borders on every panel and popup, a truecolor
  gradient header logo, and an eighth-block progress bar with a gradient fill
  and a smooth head instead of a flat gauge.
- **Artwork cache eviction:** covers are now tracked by access time and the
  least-recently-used files are deleted once the cache passes 128 MiB
  (override with `MFE_ARTWORK_CACHE_MB`). `mfe cache` shows the limit.
- **`mfe update`:** asks GitHub for the latest release, compares it
  numerically and on Unix replaces the running binary from the platform
  tarball. `--check` reports without touching anything; offline or weird
  installs fall back to the installer one-liner.
- **One-command installer:** `install.sh` opens with an animated truecolor
  MFE PLAYER wordmark, detects the distro's package manager, installs mpv,
  python3, yt-dlp, ffmpeg, cava and playerctl, then installs the player from a
  release tarball, cargo or npm, writes completions and runs `mfe doctor`.
- **npm/bun package:** a tiny launcher plus per-platform optionalDependencies
  (linux x64/arm64, linux x64 musl, macOS x64/arm64, Windows x64), so there is
  no postinstall download and bun works for free.
- **pip / pipx / uv wheels:** `pyproject.toml` uses maturin `bindings = "bin"`,
  putting the real binary on PATH with no Python glue.
- **Release pipeline:** `.github/workflows/release.yml` builds the six
  binaries, publishes the GitHub release, npm packages, PyPI wheels and the
  crates.io crate from the same tag.
- **`mfe doctor`:** checks mpv, python3, yt-dlp, ffmpeg, cava, playerctl,
  config parsing, installed plugins, mpv socket, daemon, clipboard, wallpaper
  detection and truecolor support, then prints the exact fix for anything
  missing.
- **`mfe cache` / `mfe cache clean`:** report and clear the artwork cache
  without touching downloaded audio.
- **Packaging:** AUR-style `packaging/PKGBUILD`, a Nix flake, a Homebrew
  formula template and a systemd `--user` unit for the daemon.
- **Portable transport:** mpv IPC now sits behind `src/ipc.rs` — Unix sockets
  on Linux/macOS, named pipes on Windows. Process liveness uses `kill(pid, 0)`
  instead of `/proc`, paths honour `APPDATA`/`LOCALAPPDATA`/`Library/Caches`,
  plugins run through `python3` where there are no shebangs, and the clipboard
  probes `pbpaste`/`clip` too. CI now builds Linux, macOS and (experimental)
  Windows.

## v0.2.0 — Music For Everyone

The rename release. `nuclear-tui` is now **mfe-player**. Old configs are migrated
automatically on first launch.

### Added

- **Real background playback daemon.** `Ctrl+B` now hands playback to a
  headless process that keeps the queue, repeat, shuffle, radio, MPRIS and
  prefetch alive after the TUI exits. Reattach with `mfe-player`; control it
  with `mfe-player status|toggle|stop`.
- **Six new themes:** catppuccin, dracula, tokyo-night, solarized, amber and
  ocean (12 total, `Alt+T`).
- **Playback speed:** `[` / `]` for 0.5×–2.0× and `\` to reset. Persisted
  across sessions.
- **Sleep-timer fade-out:** the last 15 seconds ramp the volume down instead of
  cutting mid-chorus.
- `install.sh` one-command build/install.
- CI and tagged release workflows.

### Fixed

- Repeat and queue no longer die when the TUI is closed (the old detach left
  `mpv` playing a single file with no advancement logic).
- Background daemon now resumes an idle queue after a handoff race.
- Stale mpv sockets no longer make `status` claim a dead player is attached.
- `stop` cleans up the socket and session lock.

### Changed

- Binary/package/config paths renamed to `mfe-player` / `~/.config/mfe-player`
  / `~/.cache/mfe-player`.
- MPRIS identity is now `org.mfeplayer.MfePlayer`.
