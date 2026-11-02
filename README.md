# mfe-player 🎧

> **fuck subscriptions. own your music.**

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
---
```

## wtf is this?

look. i got sick of paying fifteen bucks a month just to rent music while dealing with bloated electron apps eating gigabytes of ram, ads interrupting my coding flow, and getting logged out of random web players. 

so i built **mfe-player** (Music For Everyone). 

it's a fast-as-fuck terminal music player written in Rust. no ads, no telemetry, no subscription garbage. it gives you Spotify-level search and playback inside an fzf-style TUI that lives right where you already work.

---

## why use it instead of the usual garbage?

- **zero ram hogging**: runs on Rust + ratatui. uses practically nothing.
- **plugin-driven**: python stdlib plugins fetch metadata, streams, and lyrics without bloat. if one provider breaks, it auto-fails over to the next.
- **cava visualizer (`Ctrl+V`)**: real-time audio spectrum bars that actually look sick in your terminal.
- **offline downloads (`d`)**: hit `d` and it saves the track locally as Opus with full metadata so you can listen offline.
- **paste links (`Ctrl+L`)**: got a YouTube link, SoundCloud track, playlist, or remote `.m3u`? Paste it in and it just plays.
- **background detach (`Ctrl+B`)**: hit `Ctrl+B` and detach the TUI. mpv keeps blasting music in the background even if you close your terminal window.
- **mpris & media keys**: works out of the box with `playerctl` and your keyboard media keys.
- **6 color themes (`Alt+T`)**: gruvbox, nord, matrix, rose, mono, and nuclear. cycles live without wiping your config comments.

---

## what you need

- Linux (mpv IPC runs on unix sockets)
- Rust 1.85+
- `mpv` (for audio playback)
- `yt-dlp` (for streaming & ripping downloads)
- `python3` (standard library only)
- `cava` (optional, for the visualizer)
- `playerctl` (optional, for media keys)

---

## how to build & run

```bash
git clone https://github.com/omkargs/mfe-player.git
cd mfe-player
cargo build --release
install -Dm755 target/release/mfe-player ~/.local/bin/mfe-player
```

run it:
```bash
mfe-player
# or search straight from the terminal
mfe-player daft punk
```

---

## hotkeys (memorize these)

| Key | What it does |
|---|---|
| `/` or `Ctrl+F` | search / filter list |
| `Enter` | play track / open artist or album |
| `Space` | play / pause |
| `n` / `p` | next / prev track |
| `+` / `-` | volume up / down (`m` to mute) |
| `f` / `F` | favorite track |
| `d` | download track offline |
| `Ctrl+L` | import song or playlist from link |
| `Ctrl+T` | radio autoplay (never stops playing) |
| `Ctrl+V` | toggle cava spectrum visualizer |
| `Alt+T` | cycle color themes live |
| `Alt+U` | undo last delete |
| `Alt+E` / `Alt+I` | export / import M3U playlists |
| `z` | sleep timer |
| `E` | cycle EQ presets |
| `Alt+N` | toggle loudness normalization |
| `Ctrl+B` | detach TUI (keep music playing in bg) |
| `Ctrl+P` or `:` | command palette |
| `?` | help menu |
| `q` | quit |

---

## config & files

first run sets up everything cleanly:
- Config: `~/.config/mfe-player/config.toml`
- State: `~/.config/mfe-player/state.json`
- Plugins: `~/.config/mfe-player/plugins/`
- Downloads: `~/.cache/mfe-player/downloads/`

mit license. fork it, break it, fix it.
