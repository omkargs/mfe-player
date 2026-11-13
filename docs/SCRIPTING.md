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
mfe doctor                  # why is the audio not playing?
mfe cache                   # sizes + the cache limit
mfe cache clean             # free the artwork cache
mfe update                  # grab the latest release (--check to only look)
```

---
