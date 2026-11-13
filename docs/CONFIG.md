# Configuration

> Part of the [MFE Player](../README.md) manual.

Every `config.toml` key is optional; this page lists them and where each file lives.

---

## ⚙️ Config

`~/.config/mfe-player/config.toml` — every key is optional.

```toml
theme = "mfe"             # or "wallpaper" to sample your desktop
transparent = false       # Alt+W; let the terminal show what is behind it
wallpaper = ""            # explicit image for theme = "wallpaper"; empty = detect
seek_step = 5
long_seek_step = 30
volume_step = 5
max_volume = 130          # ceiling for +/- and the clickable meter
speed_step = 0.1          # [ and ] speed step
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
listenbrainz_token = ""       # optional; empty = nothing ever leaves your machine
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
