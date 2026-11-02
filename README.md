# 🎧 mfe-player (Music For Everyone)

> **Escape subscription hell.** Own your music, your workflow, and your terminal.

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

## 💀 The Problem: Subscription Hell & Bloatware

Let's be real. You're a developer. You want to listen to music while you hack, write code, or read logs. But what are your options?
- **Streaming monopolies**: $11–$17/month just to rent compressed audio, suffer algorithm fatigue, endless UI redesigns, telemetry, and ads.
- **Bloated Electron apps**: Spotify desktop client eating 1.5 GB of RAM just to render a web app that lags when you type.
- **Web players**: Getting logged out, browser tab suspended, memory leaks, and zero terminal integration.

You live in the terminal. Your tools are fast, deterministic, lightweight, and yours. Your music player should be too.

---

## ✨ Enter `mfe-player`

**mfe-player** (Music For Everyone) is a lightning-fast, plugin-driven fzf-style TUI music streaming powerhouse built in **Rust** (`ratatui` + `crossterm`) with **Python stdlib plugins**, **persistent mpv**, and absolute zero corporate telemetry.

No subscription walls. No ads. No bloat. Just pure audio speed.

---

## 🚀 Killer Features

### 🔍 1. Smart Multi-Section Search & Context Autoplay
- One query searches **artists**, **albums**, and **songs** simultaneously.
- Debounced (260ms), cached, and lightning-fast.
- Pressing `Enter` on a track anywhere queues the rest of its context (album, artist top tracks, playlist) and keeps playing seamlessly.

### 🌊 2. Streaming Handoff & Bulletproof Resilience
- Powered by installable Python plugins (`yt-dlp`, SoundCloud, YouTube, MusicBrainz, iTunes, LRC Lib).
- Every stream URL is range-probed in parallel before touching `mpv`.
- If a provider goes down or serves a dead link, `mfe-player` automatically fails over down the line so your music never skips a beat.

### 🎛️ 3. The Cava Spectrum Panel & 6 Live Themes
- Real-time `cava` visualizer panel sitting on the right (wide) or left sidebar (narrow), toggled instantly with `Ctrl+V`.
- Cycle through 6 gorgeous themes live (`nuclear`, `gruvbox`, `nord`, `mono`, `matrix`, `rose`) with `Alt+T` — your config file comments are preserved untouched.

### 📻 4. Radio Autoplay & Prefetch
- Toggle radio autoplay with `Ctrl+T`. When your queue ends, `mfe-player` automatically generates a smart radio station based on the active artist and keeps the vibe going.
- Background prefetching caches the next track while you listen for instant zero-latency track switching and gapless playback (`--gapless-audio=yes`).

### 📥 5. Offline Downloads & Link Importing (`Ctrl+L`)
- Press `d` to rip and save tracks offline into `~/.cache/mfe-player/downloads/` as Opus files with full JSON metadata sidecars. Offline copies are automatically prioritized over streaming.
- Press `Ctrl+L` to import single tracks, full YouTube/SoundCloud playlists, or remote `.m3u` files instantly.

### 🕹️ 6. MPRIS, Media Keys & Background Detach (`Ctrl+B`)
- Full MPRIS server integration (`org.mfeplayer.MfePlayer`) out of the box — control playback with your keyboard media keys or `playerctl`.
- Detach the TUI with `Ctrl+B`: `mpv` keeps streaming in the background even if you close your terminal. Reattach anytime, or run multiple independent sessions.

### 🛡️ 7. Power Tools & Undo Stack (`Alt+U`)
- Multi-select items (`v` mark single, `V` mark all) for bulk queueing, playlist addition, or downloading.
- Accidentally deleted something? Hit `Alt+U` to pop it right back from the undo stack.
- Export and import your playlists to standard M3U files (`Alt+E` / `Alt+I`).

---

## 🛠️ Installation

### Prerequisites
- **Linux** (mpv IPC uses Unix sockets)
- **Rust 1.85+** (`rustc`, `cargo`)
- **`mpv`** (audio playback engine)
- **`yt-dlp`** (for streaming and downloads)
- **`python3`** (standard library only, for plugins)
- **`cava`** (optional, for the spectrum visualiser)
- **`playerctl`** (optional, for media keys)

### Build & Install
```bash
git clone https://github.com/yourusername/mfe-player.git
cd mfe-player
cargo build --release
install -Dm755 target/release/mfe-player ~/.local/bin/mfe-player
```

---

## ⌨️ Essential Hotkeys

| Key | Action |
|---|---|
| `/` or `Ctrl+F` | Filter list / live search |
| `Enter` | Play track / open artist or album page |
| `Space` | Play / Pause |
| `n` / `p` | Next / Previous track |
| `+` / `-` | Volume up / down (`m` to mute) |
| `s` | Toggle shuffle |
| `r` | Toggle repeat |
| `f` / `F` | Toggle favorite / add to favorites |
| `d` | Download track for offline playback |
| `Ctrl+L` | Import track or playlist from URL |
| `Ctrl+T` | Toggle radio autoplay |
| `Ctrl+V` | Toggle Cava audio spectrum panel |
| `Alt+T` | Cycle color themes live |
| `Alt+U` | Undo last deletion |
| `Alt+E` / `Alt+I` | Export / Import playlists (M3U) |
| `z` | Sleep timer |
| `E` | Cycle EQ presets (flat, bass, vocal, night, loud) |
| `Alt+N` | Toggle loudness normalisation |
| `Ctrl+B` | Detach TUI (keep playing in background) |
| `Ctrl+P` / `:` | fzf-style command palette |
| `?` | Toggle help overlay |
| `q` | Quit |

---

## ⚙️ Configuration & Files

On first launch, `mfe-player` automatically sets up its default plugins and configuration:

- **Config**: `~/.config/mfe-player/config.toml`
- **State & Session**: `~/.config/mfe-player/state.json`
- **Plugins**: `~/.config/mfe-player/plugins/`
- **Offline Downloads**: `~/.cache/mfe-player/downloads/`
- **M3U Export**: `~/.config/mfe-player/mfe-player.m3u`

---

## 🤝 Contributing & License

Built by developers, for developers. PRs, bug reports, and new plugins are welcome! 

Licensed under the MIT License.
