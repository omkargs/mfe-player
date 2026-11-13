# Features

> Part of the [MFE Player](../README.md) manual.

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

