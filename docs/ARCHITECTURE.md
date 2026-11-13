# How it works

> Part of the [MFE Player](../README.md) manual.

How the TUI, daemon, plugins and mpv fit together.

---

## 🧱 How it works

```text
┌──────────────┐     events      ┌──────────────┐
│   ratatui    │ ◀─────────────▶ │  app state   │
│   TUI / daemon│                │ queue · radio│
└──────┬───────┘                 └──────┬───────┘
       │ stdin / keys                   │ JSON over Unix socket
       ▼                                ▼
┌──────────────┐                 ┌──────────────┐
│ plugin worker│ ── metadata ──▶ │     mpv      │
│ python3      │ ── stream URL ─▶│ audio engine │
└──────────────┘                 └──────────────┘
```

- Rust for the UI, state machine and IPC.
- `mpv` for actual audio: gapless, prefetch, filters, MPRIS-friendly.
- Python plugins for anything that touches the internet, because providers change faster than compiled binaries.
- One shared socket; a second TUI gets a private player instead of hijacking your music.

---
