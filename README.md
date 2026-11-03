<div align="center">

<img src="assets/banner.svg" alt="MFE Player — Music For Everyone" width="100%">

### Music For Everyone.

**A terminal music player that streams, queues, downloads and visualises — with no subscription, no account, and no Electron tab eating your RAM.**

[![CI](https://github.com/omkargs/mfe-player/actions/workflows/ci.yml/badge.svg)](https://github.com/omkargs/mfe-player/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/omkargs/mfe-player?color=ff5f94)](https://github.com/omkargs/mfe-player/releases)
[![Stars](https://img.shields.io/github/stars/omkargs/mfe-player?style=flat&color=ffd166)](https://github.com/omkargs/mfe-player/stargazers)
[![License: MIT](https://img.shields.io/badge/license-MIT-7ee787.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-f74c00.svg)](https://www.rust-lang.org)

[Why](#-the-2am-origin-story) · [Features](#-what-you-actually-get) · [Install](#-install-it) · [Keys](#-hotkeys) · [Themes](#-themes) · [Plugins](#-plugins-are-just-python) · [Roadmap](#-roadmap)

</div>

---

## ☕ The 2 a.m. origin story

I was paying for a music subscription, a "hi-fi" add-on, and somehow still getting ads for a product I already bought. The desktop app used more RAM than my editor, my browser and my whole dev environment combined. Every time I wanted to play one song, I had to leave the terminal, find a window, wait for a spinner, and pretend this was a good experience.

Then I thought: **I already live in the terminal. Why doesn't my music?**

So I built it. Rust + `ratatui` + `mpv` + Python plugins. It started as a "can I search and play a song without opening a browser" experiment. Then came the queue, favourites, playlists, lyrics, radio, downloads, MPRIS, a cava spectrum panel, and a background daemon that refuses to die when your SSH session does.

No account. No telemetry. No "sign in to continue listening". Just a search box, your music, and every key you need.

This is **MFE Player** — **M**usic **F**or **E**veryone.

> If this saves you one subscription, one laggy tab, or one "sorry, this content is not available in your region", that's the whole point.

---

## 🚀 What you actually get

| | |
|---|---|
| **fzf-style search** | Songs, artists and albums in one fuzzy list. `Enter` opens an artist/album page or starts playback. |
| **Spotify-ish queue** | Queue next, bulk-queue a whole view, reorder with `J`/`K`, jump to now playing, clear upcoming, undo mistakes. |
| **Plugins, not lock-in** | Metadata, streaming and lyrics are separate Python 3 plugins. Provider breaks? The player walks down the list and keeps going. |
| **Real background playback** | `Ctrl+B` detaches into a headless daemon. The queue, repeat, shuffle and radio keep running after the TUI is gone. |
| **Offline downloads** | `d` rips a track to disk and prefers the local copy next time. Planes and bad Wi-Fi are supported. |
| **Link importing** | `Ctrl+L` accepts a YouTube/SoundCloud track or playlist, or a remote `.m3u`. Paste and go. |
| **Radio autoplay** | `Ctrl+T` seeds a never-ending station from the current track so the silence never wins. |
| **MPRIS / media keys** | Works with `playerctl`, keyboard media keys and desktop widgets — even while detached. |
| **Album art in the terminal** | Cover art renders as truecolor half-blocks in the inspector — no sixel or Kitty protocol required. |
| **Cava visualiser** | `Ctrl+V` toggles a smoothed spectrum panel that dances to the actual audio. |
| **12 themes** | `Alt+T` cycles live, and **preserves the comments in your config file** instead of nuking them. |
| **Sleep timer** | `z` fades out over the last 15 seconds and pauses. No mid-chorus guillotine. |
| **Playback speed** | `[` / `]` for 0.5×–2.0×, `\` to reset. Podcasts, mixes and 10-hour loops welcome. |
| **Smart fade** | `Alt+F` ramps the volume down over the last seconds of a song and back up on the next one. No drops, no guillotines. |
| **ReplayGain** | `Alt+R` cycles off/track/album, so your own library plays at a sane loudness without dynamic-range nuking. |
| **Editable EQ** | `Ctrl+E` opens a 10-band equaliser (31 Hz–16 kHz). Nudge bands with ↑/↓, save with Enter. |
| **M3U import/export** | `Alt+E` / `Alt+I` so your playlists can escape this app too. |
| **Share without an account** | `Y` (or `mfe share`) copies the current track to your clipboard. No OAuth, no social graph, no "sign in to continue". |
| **History & stats** | Every play is remembered (`7`). `Ctrl+G` shows listening time, top artists and most-played tracks. |
| **Your own files** | Point `library_paths` at real folders and press `8`. Tags and embedded cover art are read with `lofty`; no upload, no account, no phoning home. |
| **Internet Archive** | A second metadata plugin searches 800 years of free music, live sets and public-domain recordings on archive.org. |
| **Radio Browser** | Search live stations worldwide with the `Radio` plugin and pipe them straight into mpv (ICY metadata and all). |

---

## 📺 What it looks like

```text
┌ library ──────┐ ┌ ⌕ search · iTunes Search ────────────────────┐ ┌ inspect ───────┐
│ 1 ⌕ search 30 │ │ ⌕  daft punk▌       Enter search/play       │ │ Get Lucky      │
│ 2 ≡ queue   4 │ │ ─────────────────────────────────────────── │ │ Daft Punk      │
│ 3 ★ favorites │ │ ▶ Get Lucky — Daft Punk        4:08         │ │ Random Access… │
│ 4 ▤ playlists │ │   Instant Crush — Daft Punk     5:37         │ │ ★ favorite     │
│ 5 ⤓ downloads │ │   Lose Yourself to Dance        5:53         │ │ y lyrics       │
│ 6 ⚙ plugins   │ │   One More Time — Daft Punk     5:20         │ │ d download     │
└───────────────┘ └─────────────────────────────────────────────┘ └────────────────┘
│ ▶ Get Lucky  Daft Punk                       2:13 / 4:08  vol 70%  repeat all │
└ / search · Enter play · a queue · F favorite · P playlist · ? help · Ctrl+P palette ┘
```

Terminal font does the rest. It looks even better with a real cava panel on the right.

---

## 🛠 Install it

You need **Linux** (mpv talks over Unix sockets), **Rust 1.85+**, **mpv**, **yt-dlp** and **python3**.

```bash
# Debian / Ubuntu
sudo apt install mpv python3 ffmpeg cava playerctl
cargo install yt-dlp   # or: pipx install yt-dlp
```

Then build and drop it on your PATH:

```bash
git clone https://github.com/omkargs/mfe-player.git
cd mfe-player
cargo build --release
install -Dm755 target/release/mfe-player ~/.local/bin/mfe-player
ln -sf ~/.local/bin/mfe-player ~/.local/bin/mfe
```

```bash
mfe                     # open the TUI
mfe daft punk           # open with search results waiting
mfe status              # what's playing?
mfe toggle              # play/pause the background player
mfe stop                # stop playback and the daemon
```

First launch writes sane defaults to `~/.config/mfe-player/` and installs the bundled plugins. It also migrates an old `nuclear-tui` config automatically, because losing your queue to a rename is a crime.

---

## ⌨️ Hotkeys

Press `?` inside the app for the full, searchable map. The ones you'll actually use:

| Key | Action |
|---|---|
| `/` · `Ctrl+F` | Search · fuzzy-filter the current view |
| `Enter` | Play · open artist/album · open playlist |
| `Space` | Play/pause (also resumes a restored queue) |
| `n` / `b` | Next / previous track |
| `←` / `→` · `Shift+←/→` | Seek −5s / +5s · −30s / +30s |
| `+` / `-` · `m` | Volume · mute |
| `[` / `]` · `\` | Speed down/up · reset to 1× |
| `s` · `r` | Shuffle · cycle repeat (off → all → one) |
| `a` · `A` | Queue selected · queue everything in view |
| `N` | Play selected next |
| `v` / `V` | Mark one / mark all |
| `f` or `F` | Favorite/unfavorite |
| `P` | Add selection to playlist |
| `J` / `K` | Reorder in queue or playlist |
| `S` | Save queue as a playlist |
| `Alt+K` · `Alt+U` | Clear upcoming · undo last removal |
| `Alt+E` · `Alt+I` | Export · import M3U |
| `d` · `5` | Download · downloads view |
| `7` · `Ctrl+G` | Listening history · listening stats |
| `8` · `R` | Local library · rescan folders |
| `Y` | Copy now playing to the clipboard |
| `Alt+F` · `Alt+R` | Smart fade cycle · replaygain cycle |
| `Ctrl+E` | Edit the 10-band EQ |
| `Ctrl+L` | Import song/playlist from a link |
| `Ctrl+T` | Toggle radio autoplay |
| `Ctrl+V` | Toggle cava spectrum |
| `z` | Sleep timer (15–90 min or end of track, fades out) |
| `E` · `Alt+N` | EQ presets · loudness normalisation |
| `Alt+T` | Cycle themes |
| `Ctrl+B` | Detach and keep playing |
| `Ctrl+P` or `:` | Command palette — every action, fuzzy-searchable |
| `q` | Quit and stop |

---

## 🌙 The background daemon (yes, repeat keeps working)

Most TUI players "detach" by leaving `mpv` playing one file and hoping you don't notice. When that song ends, the queue is dead.

MFE doesn't do that. `Ctrl+B` starts a tiny headless copy of the player that owns the socket, advances tracks, honours repeat/shuffle/radio, keeps MPRIS alive and persists state. Close the terminal, lock the laptop, drop the SSH connection — the music is still yours.

```bash
Ctrl+B          # TUI exits, daemon takes over
mfe status      # Bad (2012 Remaster) — Michael Jackson [background daemon, repeat all]
mfe toggle      # pause/resume from a script or keybind
mfe             # reattach the full UI right where playback is
mfe stop        # kill the music (and the daemon)
```

That's also why `repeat one` and radio now survive in the background instead of quietly dying after one track.

---

## 🧩 Status bars & scripts

`mfe` speaks JSON, so it drops straight into waybar, polybar, tmux or a shell prompt.

```bash
mfe status --json
```

```json
{"text":"▶ Get Lucky — Daft Punk","class":"playing","title":"Get Lucky","artist":"Daft Punk","position":73.2,"duration":248,"paused":false,"repeat":"all","shuffle":false,"mode":"daemon","connected":true}
```

Waybar module:

```json
"custom/mfe": {
  "exec": "mfe status --json",
  "return-type": "json",
  "interval": 1,
  "format": "{}",
  "on-click": "mfe toggle",
  "on-click-right": "mfe stop"
}
```

One-liners:

```bash
mfe current                 # ▶ Get Lucky — Daft Punk
mfe share                   # copy now playing to the clipboard
mfe toggle                  # pause/resume
mfe completions bash        # also: zsh, fish
```

---

## 🎨 Themes

`Alt+T` cycles live and writes the choice back to `config.toml` **without deleting your comments**.

`mfe` · `gruvbox` · `nord` · `mono` · `matrix` · `rose` · `catppuccin` · `dracula` · `tokyo-night` · `solarized` · `amber` · `ocean`

Set one permanently:

```toml
theme = "tokyo-night"
```

No theme fits? Copy `src/theme.rs`, add 13 colours, open a PR. It's genuinely that small.

---

## 🔌 Plugins are just Python

Drop a `.py` file into `~/.config/mfe-player/plugins/`. It gets discovered, loaded and kept up to date on launch. Standard library only — no virtualenv, no build step, no dependency hell.

The bundled ones:

- **metadata** — iTunes Search API, MusicBrainz
- **streaming** — YouTube (`yt-dlp`), SoundCloud (`yt-dlp`)
- **lyrics** — LRCLIB with a lyrics.ovh fallback

If one provider rate-limits you, MFE fails over to the next plugin instead of showing you a spinner forever. Adding a new source is a JSON contract, not a framework.

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

## ⚙️ Config

`~/.config/mfe-player/config.toml` — every key is optional.

```toml
theme = "mfe"
seek_step = 5
long_seek_step = 30
volume_step = 5
prefetch = true
radio = false
mouse = true
cava = true
cava_position = "right"   # right | left
cava_bars = 28
normalize = false
eq = "flat"               # flat | bass | vocal | night | loud
audio_device = ""         # e.g. "alsa/hdmi:CARD=HDMI"
fade_seconds = 0          # 2-4 = gentle crossfade feel, 0 = off
replaygain = "off"        # off | track | album
eq_gains = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]
mpris = true
library_paths = ["~/Music"]   # scanned on launch; press R in the library view to rescan
```

| Path | What lives there |
|---|---|
| `~/.config/mfe-player/config.toml` | settings |
| `~/.config/mfe-player/state.json` | queue, playlists, favorites, position |
| `~/.config/mfe-player/plugins/` | provider plugins |
| `~/.cache/mfe-player/downloads/` | offline audio |
| `~/.config/mfe-player/library.json` | local library index |
| `~/.cache/mfe-player/artwork/` | downloaded and embedded cover cache |
| `~/.config/mfe-player/mfe-player.m3u` | exported playlists |

Want scrobbles without handing over a social graph? Paste a ListenBrainz user token into `listenbrainz_token` in the config. Plays are submitted from a background thread; if the token is empty, nothing leaves your machine.

---

## 🗺 Roadmap

- [x] Search, playlists, queue, favourites, lyrics
- [x] Radio autoplay and gapless prefetch
- [x] Offline downloads and local-first playback
- [x] MPRIS, media keys, cava, EQ, normalisation
- [x] Sleep timer with fade-out
- [x] Background daemon with working repeat/queue
- [x] 12 themes and playback speed
- [ ] Crossfade between tracks
- [ ] Scrobbling plugin (ListenBrainz / Last.fm)
- [ ] Windows/macOS named-pipe transport
- [ ] Plugin marketplace UI

PRs for any unchecked box are welcome. So are issues that start with "this broke at 3 a.m." — those are usually the real ones.

---

## 🤝 Contributing

1. Fork it.
2. `cargo build --release`
3. Break something.
4. Fix it better than I would have.
5. Open a PR with a screenshot and a one-line reason.

Read [CONTRIBUTING.md](CONTRIBUTING.md) for the boring-but-useful details. Bug reports with the output of `mfe status`, your distro and `mpv --version` get fixed about 10× faster.

If MFE saves you money, time or one browser tab: **drop a star**. Stars are how other devs find the thing, and how I know someone besides me is using it.

---

<div align="center">

**Built because renting music is a weird business model.**

MIT licensed. No ads. No trackers. No "upgrade to continue".

⭐ [Star the repo](https://github.com/omkargs/mfe-player) · 🐛 [Report a bug](https://github.com/omkargs/mfe-player/issues) · 💡 [Request a feature](https://github.com/omkargs/mfe-player/issues)

</div>
