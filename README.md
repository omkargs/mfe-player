<div align="center">

<img src="assets/banner.svg" alt="MFE Player — Music For Everyone" width="100%">

<img src="docs/demo.gif?v=3" alt="MFE Player in action — search, queue, spectrum, lyrics, detach to daemon" width="100%">

### Music For Everyone.

**A terminal music player that streams, queues, downloads and visualises — with no subscription, no account, and no Electron tab eating your RAM.**

[![CI](https://github.com/omkargs/mfe-player/actions/workflows/ci.yml/badge.svg)](https://github.com/omkargs/mfe-player/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/omkargs/mfe-player?color=ff5f94)](https://github.com/omkargs/mfe-player/releases)
[![Stars](https://img.shields.io/github/stars/omkargs/mfe-player?style=flat&color=ffd166)](https://github.com/omkargs/mfe-player/stargazers)
[![License: MIT](https://img.shields.io/badge/license-MIT-7ee787.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-f74c00.svg)](https://www.rust-lang.org)

[The rant](#-the-rant) · [Features](#-what-you-actually-get) · [Install](docs/INSTALL.md) · [Keys](docs/KEYS.md) · [Daemon](docs/DAEMON.md) · [Themes](docs/THEMES.md)

```bash
curl -fsSL https://raw.githubusercontent.com/omkargs/mfe-player/master/install.sh | bash
```

**Platforms:** Linux is the tested, everything-works target · macOS builds in CI and is solid · **Windows is experimental** — the native build exists but nobody has certified it on real hardware, so on Windows the reliable path is Docker (`docker compose run --rm mfe`).

</div>

---

## The reason why i build this : 

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

The short list — the complete one lives in [docs/FEATURES.md](docs/FEATURES.md).

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

→ **The other 20+**, screenshots of the layout and the fine print: [docs/FEATURES.md](docs/FEATURES.md)

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

## 📚 The rest of the manual

Everything longer than a tweet lives in [docs/](docs/):

| Doc | What's in it |
|---|---|
| [Install](docs/INSTALL.md) | every install channel, platforms, Docker, first steps |
| [Keys](docs/KEYS.md) | every hotkey, the full map |
| [Daemon](docs/DAEMON.md) | background playback and the CLI verbs that drive it |
| [Scripting](docs/SCRIPTING.md) | waybar/polybar/tmux JSON, one-liners |
| [Themes](docs/THEMES.md) | 14 built-ins, wallpaper palettes, transparency |
| [Plugins](docs/PLUGINS.md) | the Python JSON contract, bundled providers |
| [Architecture](docs/ARCHITECTURE.md) | how the pieces talk to each other |
| [Features](docs/FEATURES.md) | the complete feature table |
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
