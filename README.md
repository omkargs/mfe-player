<div align="center">

# 🎧 mfe-player (Music For Everyone)

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/platform-linux-blue.svg)]()
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)]()

> **Escape subscription hell.** Own your music, your workflow, and your terminal.

[Features](#-why-use-it-instead-of-the-usual-garbage) · [Installation](#-how-to-install) · [Hotkeys](#️-hotkeys-memorize-these) · [Plugins](#-plug-in-your-own-stuff)

</div>

```
┌ library ──────┐ ┌ ⌕ search · iTunes Search ────────────────────┐ ┌ inspect ───────┐
│ 1 ⌕ search 30 │ │ ⌕  daft punk▌       Enter search/play       │ │ Get Lucky      │
│ 2 ≡ queue   4 │ │ ─────────────────────────────────────────── │ │ Daft Punk      │
│ 3 ★ favorites │ │ ▶ Get Lucky — Daft Punk        4:08         │ │ Random Access… │
│ 4 ▤ playlists │ │   Instant Crush — Daft Punk     5:37         │ │ ★ favorite     │
│ 5 ⚙ plugins 5 │ │   Lose Yourself to Dance        5:53         │ │ y lyrics       │
└───────────────┘ └─────────────────────────────────────────────┘ └────────────────┘
│ ▶ Get Lucky  Daft Punk                        2:13 / 4:08   vol 70%  repeat off │
└ / search · Enter play · a queue · F favorite · P playlist · y lyrics · ? help ┘
```

---

## wtf is this?

look. i got sick of paying fifteen bucks a month just to rent compressed audio while dealing with bloated electron apps eating gigabytes of ram, ads interrupting my coding flow, and getting logged out of random web players. 

so i built **mfe-player** (Music For Everyone). 

it's a fast-as-fuck terminal music player written in Rust. no ads, no telemetry, no subscription garbage. it gives you Spotify-level search, streaming, lyrics, and downloads inside an fzf-style TUI that lives right where you already work.

---

## ⚡ why use it instead of the usual garbage?

- **zero ram hogging**: built in Rust with `ratatui`. uses a fraction of the RAM a web browser tab takes.
- **plugin-driven freedom**: python stdlib plugins fetch metadata, streams, and lyrics. if one provider goes down or blocks you, `mfe-player` automatically fails over to the next.
- **real-time cava visualizer (`Ctrl+V`)**: smoothed audio spectrum bars that pulse to your music directly inside the TUI.
- **offline downloads (`d`)**: rip any track instantly into local Opus files with JSON metadata sidecars. offline copies are prioritized automatically.
- **paste any link (`Ctrl+L`)**: got a YouTube link, SoundCloud track, whole playlist, or remote `.m3u`? Paste it in and it just plays.
- **background detach (`Ctrl+B`)**: hit `Ctrl+B` to detach the TUI. `mpv` keeps streaming in the background even if you close your terminal window. reattach anytime.
- **mpris & media keys**: controls seamlessly out of the box with `playerctl` and your keyboard media keys.
- **6 live color themes (`Alt+T`)**: gruvbox, nord, matrix, rose, mono, and nuclear. cycles live without ever wiping your config comments.

---

## 🛠️ system requirements

- **Linux** (mpv IPC runs over Unix sockets)
- **Rust 1.85+** (`rustc`, `cargo`)
- **`mpv`** (audio playback engine)
- **`yt-dlp`** (streaming & ripping downloads)
- **`python3`** (standard library only)
- **`cava`** (optional, for the spectrum visualizer)
- **`playerctl`** (optional, for desktop media keys)

---

## 📦 how to install

```bash
git clone https://github.com/omkargs/mfe-player.git
cd mfe-player
cargo build --release
install -Dm755 target/release/mfe-player ~/.local/bin/mfe-player
ln -sf ~/.local/bin/mfe-player ~/.local/bin/mfe
```

run it instantly:
```bash
mfe
# or query straight from CLI
mfe daft punk
```

---

## ⌨️ hotkeys (memorize these)

| Key | What it does |
|---|---|
| `/` or `Ctrl+F` | search / filter current view |
| `Enter` | play track / open artist or album page |
| `Space` | play / pause |
| `n` / `p` | next / previous track |
| `+` / `-` | volume up / down (`m` to mute) |
| `f` / `F` | toggle track favorite |
| `d` | download track for offline playback |
| `Ctrl+L` | import song or playlist from link |
| `Ctrl+T` | toggle radio autoplay (never stops) |
| `Ctrl+V` | toggle Cava audio spectrum visualizer |
| `Alt+T` | cycle color themes live |
| `Alt+U` | undo last deletion |
| `Alt+E` / `Alt+I` | export / import M3U playlists |
| `z` | sleep timer |
| `E` | cycle EQ presets (flat, bass, vocal, night, loud) |
| `Alt+N` | toggle loudness normalization |
| `Ctrl+B` | detach TUI (keep music playing in background) |
| `Ctrl+P` or `:` | fzf-style command palette |
| `?` | toggle help overlay |
| `q` | quit |

---

## 🔌 plug in your own stuff

plugins live in `~/.config/mfe-player/plugins/`. they are simple Python 3 scripts using only the standard library. Want to add a new metadata scraper or streaming source? Just drop a `.py` file in the folder — `mfe-player` discovers, loads, and auto-upgrades them on launch.

---

## 📁 config & state locations

- **Config**: `~/.config/mfe-player/config.toml`
- **State & Queue**: `~/.config/mfe-player/state.json`
- **Plugins**: `~/.config/mfe-player/plugins/`
- **Downloads**: `~/.cache/mfe-player/downloads/`
- **M3U Export**: `~/.config/mfe-player/mfe-player.m3u`

---

## ⭐ support & contribution

if you hate subscription paywalls as much as I do, **drop a star ⭐** on GitHub to help other devs find it! 

PRs, bug reports, and custom plugins are extremely welcome. Fork it, break it, fix it.

Licensed under the **MIT License**.
