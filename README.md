<div align="center">

<img src="assets/banner.svg" alt="MFE Player — Music For Everyone" width="100%">

<pre align="center">
      __  __   ______   ______
     |  \/  | |  ____| |  ____|
     | |\/| | | |__    | |__
     | |  | | |  __|   |  __|
     |_|  |_| |_|      |_|

      _____   _          _      _   _   ______   ____
     |  __ \ | |        / \    | | | | |  ____| |  _ \
     | |__) | | |      / _ \   | | |_| | |__    | |_) |
     |  ___/  | |____  / ___ \  |__   _| |  __|   |  _ <
     |_|      |______| /_/   \_\    |_|   |_|      |_| \_\

</pre>

### Music For Everyone.

**A terminal music player that streams, queues, downloads and visualises — with no subscription, no account, and no Electron tab eating your RAM.**

[![CI](https://github.com/omkargs/mfe-player/actions/workflows/ci.yml/badge.svg)](https://github.com/omkargs/mfe-player/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/omkargs/mfe-player?color=ff5f94)](https://github.com/omkargs/mfe-player/releases)
[![Stars](https://img.shields.io/github/stars/omkargs/mfe-player?style=flat&color=ffd166)](https://github.com/omkargs/mfe-player/stargazers)
[![License: MIT](https://img.shields.io/badge/license-MIT-7ee787.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-f74c00.svg)](https://www.rust-lang.org)

[The rant](#-the-rant) · [Features](#-what-you-actually-get) · [Install](#-install-it) · [Docs](docs/KEYS.md) · [Daemon](docs/DAEMON.md) · [Config](docs/CONFIG.md)

```bash
curl -fsSL https://raw.githubusercontent.com/omkargs/mfe-player/master/install.sh | bash
```

</div>

---

## 😤 The rant

I'm going to be honest with you: I built this because I got tired of being farmed.

I was paying for a music subscription. Then a "lossless" add-on. Then a family plan I didn't need. And after all that, the app still had ads — for the same service — and one song required three taps, two spinners and a page reload. The desktop client sat there using more RAM than my editor, my browser and my whole dev environment combined, to play a 4 MB MP3.

At some point I looked at my terminal, where I already live, and thought: **why is my music not here?**

Not "why isn't there a web player I can embed". Not "why isn't there a plugin for VS Code". Why is the one place I spend ten hours a day the *only* place I can't hit space and hear a song?

So I built it. Rust + `ratatui` + `mpv` + Python plugins. It started as "can I search and play a song without opening a browser". Then the queue showed up. Then favourites, playlists, lyrics, radio, downloads, MPRIS, a cava panel that actually moves to the audio, and a background daemon that keeps playing when your SSH session drops.

Some things I got tired of, specifically:

- **"Your music, anywhere."** Except on the one device I use.
- **Region locks** on songs that are, somehow, available for free elsewhere.
- **Hi-Fi tiers** so I can hear the ads in higher fidelity.
- **"Sign in to continue listening"** — to music I already paid for.
- **$11.99 a month**, forever, for a library I could have ripped in 2006 and owned.
- **4 GB of RAM** for a UI that needs one text box and a play button.

No account. No telemetry. No "recommended for you" — I know what I want to listen to, that's why I searched for it. No dark patterns. Just a search box, your music, and every key you need.

This is **MFE Player** — **M**usic **F**or **E**veryone. It's MIT-licensed, it's a terminal app, and it does not give a single shit about your engagement metrics.

> If this saves you one subscription, one laggy tab, or one "sorry, this content is not available in your region", that's the whole point.

---

## 🚀 What you actually get

| | |
|---|---|
| **fzf-style search** | Songs, artists and albums in one fuzzy list. `Enter` opens an artist/album page or starts playback. |
| **Spotify-ish queue** | Queue next, bulk-queue a whole view, reorder with `J`/`K`, jump to now playing, clear upcoming, undo mistakes. |
| **Plugins, not lock-in** | Metadata, streaming and lyrics are separate Python 3 plugins. Provider breaks? The player walks down the list and keeps going. |
| **Real background playback** | `Ctrl+B` detaches into a headless daemon. The queue, repeat, shuffle and radio keep running after the TUI is gone. |
| **Drive it from any terminal** | `mfe play`, `pause`, `toggle`, `next`, `prev`, `mute`, `seek +10`, `shuffle`, `repeat`, `volume` — bind them to anything, script anything. The queue and repeat state stay correct because the daemon, not mpv, applies them. |
| **Offline downloads** | `d` rips a track to disk and prefers the local copy next time. Planes and bad Wi-Fi are supported. |
| **Link importing** | `Ctrl+L` accepts a YouTube/SoundCloud track or playlist, or a remote `.m3u`. Paste and go. |
| **Radio autoplay** | `Ctrl+T` seeds a never-ending station from the current track so the silence never wins. |
| **Taste autoplay** | When the queue ends, a local model continues with the same vibe — learned from plays, skips and tags. Occasional probes test whether your taste moved. `mfe taste` explains every pick. |
| **MPRIS / media keys** | Works with `playerctl`, keyboard media keys and desktop widgets — even while detached. |
| **Album art in the terminal** | Cover art renders as truecolor half-blocks in the inspector — no sixel or Kitty protocol required. |
| **Cava visualiser** | `Ctrl+V` toggles a smoothed spectrum panel with fast-rise peak caps, coloured by frequency. The same energy drives every animation. |
| **14 themes** | `Alt+T` cycles live, and **preserves the comments in your config file** instead of nuking them. |
| **Looks like a modern TUI** | One hue, brightness only: a slow band of light rides the header and the played part of the progress bar, the spectrum gives each cell its own glow, titles breathe with the bass. No rainbow shimmer, no motion sickness. `Alt+M` freezes it. |
| **Clickable volume meter** | Click or drag the mini meter in the player bar; `+`/`-` respect the `max_volume` cap you set. |
| **Blends with your wallpaper** | `Alt+W` stops painting the background, so your terminal's own transparency shows the wallpaper through the app. Panels stay solid, so text stays readable. |
| **Wallpaper palette** | `theme = "wallpaper"` (or `mfe theme adopt`) samples the wallpaper your compositor is using — swww, hyprpaper, GNOME, Plasma, macOS or Windows — and builds a matching dark palette from it. |
| **Discord Rich Presence** | Optional: friends see the track, artist and progress over Discord’s local socket. No token, no server, no history leaves the machine. |
| **Sleep timer** | `z` fades out over the last 15 seconds and pauses. No mid-chorus guillotine. |
| **Playback speed** | `[` / `]` for 0.5×–2.0×, `\` to reset. Podcasts, mixes and 10-hour loops welcome. |
| **Smart fade** | `Alt+F` ramps the volume down over the last seconds of a song and back up on the next one. No drops, no guillotines. |
| **ReplayGain** | `Alt+R` cycles off/track/album, so your own library plays at a sane loudness without dynamic-range nuking. |
| **Editable EQ** | `Ctrl+E` opens a 10-band equaliser (31 Hz–16 kHz). Nudge bands with ↑/↓, save with Enter. |
| **M3U import/export** | `Alt+E` / `Alt+I` so your playlists can escape this app too. |
| **Share without an account** | `Y` (or `mfe share`) copies the current track to your clipboard. No OAuth, no social graph, no "sign in to continue". |
| **History & stats** | Every play is remembered (`7`). `Ctrl+G` shows listening time, top artists and most-played tracks. |
| **Your own files** | Point `library_paths` at real folders and press `8`. Tags and embedded cover art are read with `lofty`; no upload, no account, no phoning home. |
| **Internet Archive** | A metadata plugin searches 800 years of free music, live sets and public-domain recordings on archive.org. |
| **Radio Browser** | Search live stations worldwide with the `Radio` plugin and pipe them straight into mpv (ICY metadata and all). |
| **Cross-platform** | Linux is the tested target. macOS builds in CI with native caches and clipboard. Windows uses mpv named pipes and is experimental until someone runs it on real hardware. |
| **Install anywhere** | One installer, plus npm/bun, pip/pipx/uv wheels, cargo, GitHub releases, Nix, AUR and a Homebrew formula — all built from the same tag. |
| **Free of charge, forever** | MIT license. One binary, roughly the size of a single album cover, and it never asks for a credit card. |

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

### One command

```bash
curl -fsSL https://raw.githubusercontent.com/omkargs/mfe-player/master/install.sh | bash
```

This is the one I actually run. It opens with the MFE PLAYER wordmark, detects your package manager, installs **mpv, python3, yt-dlp, ffmpeg, cava and playerctl**, drops the player in `~/.local/bin`, writes bash/fish/zsh completions and finishes with `mfe doctor` so a broken audio stack is caught before you press space. No flags, no prompts. Linux and macOS.

### Pick your package manager

Every tagged release builds all of these from the same commit:

| Channel | Install | Also works |
|---|---|---|
| **Docker** | `docker compose up -d` | `Dockerfile` and `docker-compose.yml` included |
| **npm / bun** | `npm install -g mfe-player` | `bun add -g mfe-player`, `bunx mfe-player` |
| **pip / pipx / uv** | `pipx install mfe-player` | `uv tool install mfe-player`, `pip install mfe-player` |
| **cargo** | `cargo install mfe-player` | `cargo binstall mfe-player` (prebuilt, no compile) |
| **GitHub releases** | the tarball for your target triple | `install.sh` uses it automatically |
| **Arch** | `packaging/PKGBUILD` | builds from source; AUR push pending |
| **Nix** | `nix run github:omkargs/mfe-player` | `flake.nix` with runtime `PATH` wrapping |
| **Homebrew** | `packaging/homebrew/mfe-player.rb` | tap pending |
| **systemd** | `packaging/mfe-player.service` | `systemctl --user enable --now mfe-player` |

The npm packages use per-platform `optionalDependencies` (esbuild-style: **no postinstall download**, and bun gets the same package for free). The PyPI wheels are the real Rust binary via maturin `bindings = "bin"`, so no Python glue and no compiler at install time. `pip`/`pipx` install the `mfe-player` command; npm, cargo, Nix and the installer also give you the short `mfe` alias.

### Platforms

| Platform | Status | What it uses |
|---|---|---|
| **Linux** | tested | Unix socket, MPRIS, cava, the full stack |
| **macOS** | CI-built, same code path | Unix socket, `kill(pid, 0)`, `~/Library/Caches`, `pbcopy`/`pbpaste` |
| **Windows** | experimental | named-pipe mpv IPC, `DETACHED_PROCESS`, `tasklist`/`taskkill`, `clip`/PowerShell clipboard, plugins via `python3` |

CI builds Linux, macOS and Windows on every push. Windows is marked experimental until someone actually runs it on hardware and tells me what broke; the transport layer is already there, so it should be a short list.

### From source

```bash
git clone https://github.com/omkargs/mfe-player.git
cd mfe-player
cargo build --release --locked
install -Dm755 target/release/mfe-player ~/.local/bin/mfe-player
ln -sf ~/.local/bin/mfe-player ~/.local/bin/mfe
```

You need **Rust 1.85+**, **mpv**, **yt-dlp** and **python3** for the source build.

### Docker

```bash
docker compose build
docker compose run --rm mfe           # open the TUI
docker compose run --rm mfe doctor    # verify mpv / yt-dlp / python inside the image
```

The image bundles mpv, yt-dlp, python3, ffmpeg and cava, and runs the same Linux
binary on Linux, macOS and Windows through Docker Desktop. Config, cache and
the queue live in named volumes, so they survive `docker compose run --rm`;
the commented bind mount in `docker-compose.yml` points the container at your
music folder.

Audio is the one thing a container cannot fake. On Linux, mount the Pulse or
PipeWire socket (see the commented lines) to hear it; on macOS and Windows,
Docker Desktop has no host audio socket, so use the container for the UI,
search, playlists, `doctor` and reproducible builds rather than for listening.

### After it is installed

```bash
mfe                     # open the TUI
mfe daft punk           # open with search results waiting
mfe status              # what's playing?
mfe toggle              # play/pause the background player
mfe next                # next track (prev, mute, seek +10, shuffle, repeat)
mfe volume              # print the current volume
mfe volume 65           # set it (also +5, -5, up, down, mute)
mfe stop                # stop playback and the daemon
mfe doctor              # why is the audio not playing?
mfe theme adopt         # derive the UI colours from your wallpaper
mfe taste               # what autoplay has learned about your taste
mfe discord status      # Discord presence state (on|off|client <id>)
mfe motion on|off       # reactive animations on/off (Alt+M in the app)
mfe cache               # artwork size + LRU limit
mfe update              # grab the latest release (--check to only look)
```

First launch writes sane defaults to `~/.config/mfe-player/` and installs the bundled plugins. It also migrates an old `nuclear-tui` config automatically, because losing your queue to a rename is a crime.

Not in the official distro repos yet — the packaging files are the real thing, waiting for a tap and an AUR push.

---

## 📚 The rest of the manual

Everything longer than a tweet lives in [docs/](docs/):

| Doc | What's in it |
|---|---|
| [Keys](docs/KEYS.md) | every hotkey, the full map |
| [Daemon](docs/DAEMON.md) | background playback and the CLI verbs that drive it |
| [Scripting](docs/SCRIPTING.md) | waybar/polybar/tmux JSON, one-liners |
| [Themes](docs/THEMES.md) | 14 built-ins, wallpaper palettes, transparency |
| [Plugins](docs/PLUGINS.md) | the Python JSON contract, bundled providers |
| [Architecture](docs/ARCHITECTURE.md) | how the pieces talk to each other |
| [Config](docs/CONFIG.md) | every `config.toml` key and where files live |

---

## 📄 License

MIT. Do what you want with it — the license text is in [LICENSE](LICENSE). No
pressure either way, honestly.

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
