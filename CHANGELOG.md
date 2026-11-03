# Changelog

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
