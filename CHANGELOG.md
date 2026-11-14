# Changelog

## 0.3.3 — 2026-11-14

### Fixed

- **Closing the terminal no longer kills the music, and no longer leaves the
  UI spinning forever.** A dead pty stays "readable" and `crossterm::poll`
  loops on the EOF; UIs were found running at ~90% of a core for hours with
  zombie `cava`/`mpv` children, a stale private socket and the MPRIS name.
  Input is now polled with `poll(2)`, `POLLHUP` hands playback to the daemon,
  and the tty is verified before crossterm sees the fd.
- **Ctrl+B background handoff actually attaches now.** `session.pid` is
  validated against a real `mfe` process (pid reuse used to look like a live
  session forever), stale locks and private sockets are cleaned at startup,
  and a `handoff.pid` marker stops a UI that opens mid-handoff from grabbing
  a private player. The daemon no longer logs `previous session never
  released the player` and quits after ten seconds.
- **A second `mfe` joins the running player.** It asks the live UI to hand
  playback over and then takes the shared mpv; the same song keeps playing
  across the switch instead of a private player starting a second one.
  Ctrl+B from a private player is refused with a message rather than
  stopping the music.
- Private players die with the session that owns them instead of surviving
  as uncontrollable orphans.

### Resource safety

- Plugin and download processes are killed as a whole process group on
  timeout/cancel, and helper processes die with the player (parent-death
  signal). Superseded resolves and prefetches are cancelled, play vs
  prefetch is deduplicated, and downloads run at most two at a time.
- Bounded plugin capture (8 MiB), artwork decode (24 MiB / 4096 px), search
  results, the MPRIS update queue, scrobbles in flight and the mpv demuxer
  cache. Search/stream/lyrics/radio/probe caches have TTLs and entry caps,
  and freed heap pages are returned after heavy work.

## Unreleased

### Added

- **Clickable volume meter** in the player bar. Click or drag to set the level;
  it brightens with loudness instead of changing hue.
- **`max_volume` and `speed_step` config keys.** Both were hardcoded before;
  `max_volume` now applies to the keys, the meter, media keys, restored state
  and `mfe volume`.
- **`mfe volume [N | +N | -N | up | down | mute]`** for scripts and status bars;
  prints the value when given no argument.
- **CLI control for the background daemon:** `mfe play`, `pause`, `toggle`,
  `next`, `prev`, `mute`, `seek`, `shuffle` and `repeat` now drive the running
  daemon through a small command file, so the queue and the taste profile stay
  authoritative. Stateless verbs fall back to mpv directly when no daemon is
  running.
- **Docker.** A multi-stage `Dockerfile` and `docker-compose.yml` with mpv,
  yt-dlp, python3, ffmpeg and cava bundled, config/cache mounted, usable from
  Linux, macOS and Windows through Docker Desktop.
- `default_view` now accepts `library`, `history`, `browse`, plus the `likes`
  and `lists` aliases.

### Changed

- **Animations: one hue, brightness only.** The header rule and the played
  portion of the progress bar carry a single slow band of light; the spectrum
  and EQ meters give each cell its own brightness; titles breathe gently with
  the bass. No more rainbow sweeps or per-track hue jumps.
- Mouse: dragging now continues seeking and volume changes instead of dropping
  the gesture, and a drag never counts as a double-click.
- The help overlay reads the configured seek, volume and speed steps instead of
  hardcoded numbers.
- Narrow terminals show the volume after the clock instead of hiding it.

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
- **Motion engine.** Cava's spectrum is split into bass/mid/treble, smoothed
  with a fast attack and slow release, and drives the border glow, wave meter,
  progress head, marquee speed and track markers. `motion = false` (or `Alt+M`,
  `mfe motion off`) freezes every frame for screen readers and calm setups.

### Changed

- Platform layer (`src/platform/`) with MPRIS on Linux and inert media stubs
  elsewhere; `mpris-server` and `libc` are now platform-scoped dependencies.
- Windows: named-pipe mpv probing, `CREATE_NO_WINDOW` spawning, Python plugin
  launch and platform-aware doctor output.
- Daemon: graceful stop via a marker file and a heartbeat that makes stale pid
  files and PID reuse harmless.
- npm launcher: Alpine/musl detection, Rosetta preference, proxy-aware
  downloads and Node >= 18.
- UI polish: audio-reactive gradients, dwell-then-scroll marquee titles,
  bass-lit player and cover frames, a 10-cell spectrum wave, frequency-coloured
  analyser peaks, autoplay badges and live footer chips
  (taste/radio/discord/sleep/still).
- CLI output now exits quietly on a closed pipe instead of panicking.
- Version bumped to 0.3.0 across Cargo, PyPI and all npm packages.

### Fixed

- Closing the terminal/window (SIGHUP/SIGTERM/SIGINT, or a failed terminal
  write) now hands mpv and MPRIS to the background daemon exactly like
  `Ctrl+B`. Playback keeps going and the desktop keeps showing the track
  instead of losing its media card.
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
