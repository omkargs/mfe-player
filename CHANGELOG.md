# Changelog

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
