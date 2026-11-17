# Status bars & scripts

> Part of the [MFE Player](../README.md) manual.

`mfe` speaks JSON, so it drops straight into waybar, polybar, tmux or a shell prompt.

---

## 🧩 Status bars & scripts

`mfe` speaks JSON, so it drops straight into waybar, polybar, tmux or a shell prompt.

```bash
mfe status --json
```

```json
{"text":"▶ Get Lucky — Daft Punk","class":"playing","title":"Get Lucky","artist":"Daft Punk","position":73.2,"duration":248,"paused":false,"repeat":"all","shuffle":false,"mode":"daemon","connected":true,"spectrum":[0.0,0.08,0.31,0.62,0.44,0.19,0.07,0.0],"pulse":0.42,"energy":0.37}
```

`spectrum` is the player's own 64-band, mel-spaced visualiser (0..1.25), so a
bar can draw a **mini cava** without spawning a second capture process. It is
refreshed about seven times a second and is empty when nothing is playing.
`pulse` is the beat glow and `energy` the overall loudness, both 0..1.
[`contrib/mfe-spectrum.sh`](https://github.com/omkargs/mfe-player/blob/master/contrib/mfe-spectrum.sh)
is a ready-made `▁▂▃▄▅▆▇█` renderer for waybar/polybar/YASB.

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
mfe doctor                  # why is the audio not playing?
mfe cache                   # sizes + the cache limit
mfe cache clean             # free the artwork cache
mfe update                  # grab the latest release (--check to only look)
```

---
