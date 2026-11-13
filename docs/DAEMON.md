# The background daemon

> Part of the [MFE Player](../README.md) manual.

Ctrl+B hands playback to a headless daemon; this page shows what it can do and how the CLI drives it.

---

## 🌙 The background daemon (yes, repeat keeps working)

Most TUI players "detach" by leaving `mpv` playing one file and hoping you don't notice. When that song ends, the queue is dead.

MFE doesn't do that. `Ctrl+B` starts a tiny headless copy of the player that owns the socket, advances tracks, honours repeat/shuffle/radio, keeps MPRIS alive and persists state. Close the terminal, lock the laptop, drop the SSH connection — the music is still yours.

```bash
Ctrl+B          # TUI exits, daemon takes over
mfe status      # Bad (2012 Remaster) — Michael Jackson [background daemon, repeat all]
mfe toggle      # pause/resume from a script or keybind
mfe next        # advance the queue (prev, shuffle on|off, repeat off|all|one)
mfe mute        # toggle mute
mfe seek +10    # jump forward; negative values jump back
mfe volume 65   # absolute level, or +5 / -5 / up / down
mfe             # reattach the full UI right where playback is
mfe stop        # kill the music (and the daemon)
```

Every one of those goes through a tiny command file (`daemon.control`) that
the daemon drains on its next tick, so the queue, repeat/shuffle and taste
profile stay in one place no matter which process pressed the button. When no
daemon is running, `play`, `pause`, `toggle`, `mute`, `seek` and `volume` fall
back to talking to mpv directly; queue commands (`next`, `prev`, `shuffle`,
`repeat`) say so instead of silently doing nothing.

That's also why `repeat one` and radio survive in the background instead of quietly dying after one track. Yes, that bug annoyed me enough to build a whole daemon about it.

---
