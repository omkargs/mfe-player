# Ricing

> Part of the [MFE Player](../README.md) manual.

MFE is built to sit inside a riced desktop instead of fighting it: transparent
by default, palette-matched, beat-reactive, and scriptable from the same status
JSON your bar already polls.

This page is the copy-paste part. Everything below follows the current code and
upstream docs; the capture side still depends on your `cava` build (macOS
CoreAudio tap, Windows WASAPI), so `mfe doctor` is the quickest way to confirm
what your machine actually has.

---

## What MFE gives a rice

| Feature | Where |
|---|---|
| **Transparent chrome** | `Alt+W` (on by default). MFE stops painting the base background, so kitty/Ghostty/WezTerm/Windows Terminal opacity and blur show through. Linux, macOS and Windows. |
| **Spectrum panel** | `Ctrl+V` toggles capture, `Alt+S` removes the panel from the layout completely (no empty gutter) while the theme keeps pulsing. |
| **Theme that follows the wallpaper** | `theme = "wallpaper"`, `mfe theme adopt`, or `Alt+T` to cycle to it. |
| **Beat-reactive UI** | The in-process visualiser detects kicks and pulses borders, gradients and the player bar. `Alt+M` freezes everything. |
| **Status JSON** | `mfe status --json` includes `text`, `alt`, `class`, `tooltip`, `position`, `duration` **and** a live `spectrum` array plus `pulse` / `energy`. |
| **Mini spectrum for any bar** | [`contrib/mfe-spectrum.sh`](../contrib/mfe-spectrum.sh) turns that JSON into `▁▂▃▄▅▆▇█` bars. |

The visualiser itself runs in-process at 60 fps: mel-spaced bands, asymmetric
spring attack/release, peak hold, beat detection, 1/8-cell block glyphs. `cava`
is only the capture layer, and it now works on all three desktops:

| Platform | Capture | Notes |
|---|---|---|
| Linux | PipeWire → PulseAudio fallback | `cava` from your distro repo |
| macOS | CoreAudio | `source = tap` captures system output on macOS 14.2+; older versions fall back to `auto` (use BlackHole/Background Music if the mic is picked up) |
| Windows | WASAPI (automatic) | no `[input]` block is written; needs a recent `cava` (0.10.4+ recommended) |

> cava still needs its own audio permission on macOS (System Settings →
> Privacy & Security → Screen & System Audio Recording for the tap). That is a
> macOS requirement, not an MFE one.

---

## 🐧 Hyprland / waybar (Linux)

```bash
# Arch
sudo pacman -S cava
# Debian/Ubuntu
sudo apt install cava
```

**Transparent terminal.** In kitty's `kitty.conf`:

```conf
background_opacity 0.82
dynamic_background_opacity yes
# optional, kitty 0.40+: paint only the background cells
transparent_background_colors yes
```

Ghostty (`~/.config/ghostty/config`):

```conf
background-opacity = 0.82
background-opacity-cells = true
```

**Match the rice palette.** If the rest of the desktop is Catppuccin Mocha:

```toml
# ~/.config/mfe-player/config.toml
theme = "catppuccin"
transparent = true
cava = true
cava_position = "right"
```

Or let it follow the wallpaper (swww/hyprpaper/GNOME/Plasma are probed):

```bash
mfe theme adopt
```

The available rice staples are `catppuccin`, `catppuccin-latte`, `everforest`,
`one-dark`, `tokyo-night`, `gruvbox`, `nord`, `dracula`, `kanagawa`,
`rose-pine`, `solarized`, `ocean`, `amber`, `matrix`, `mono`, `rose`, `auto`,
`wallpaper` (full list: `mfe theme list`).

**Waybar: now-playing + mini spectrum.**

```jsonc
"custom/mfe": {
  "exec": "mfe status --json",
  "return-type": "json",
  "interval": 1,
  "format": "{text}",
  "tooltip": true,
  "on-click": "mfe toggle",
  "on-click-right": "mfe next",
  "on-scroll-up": "mfe volume +5",
  "on-scroll-down": "mfe volume -5"
},
"custom/mfe-spectrum": {
  "exec": "~/.local/bin/mfe-spectrum.sh 12",
  "return-type": "json",
  "interval": 1,
  "format": "{}",
  "tooltip": false,
  "on-click": "mfe toggle"
}
```

Copy the helper somewhere on `PATH`:

```bash
install -Dm755 contrib/mfe-spectrum.sh ~/.local/bin/mfe-spectrum.sh
```

The script reads the `spectrum` array from the player itself, so the bar and
the TUI always show the same frequencies, and it does **not** spawn another
cava process. If you prefer waybar's native `cava` module, it still works — it
just captures audio a second time.

**Cava panel position.** `cava_position = "right" | "left"` moves the panel; on
terminals narrower than 96 columns it docks under the sidebar. `cava_bars` is
the capture resolution (4–96); the panel re-bins it to any width.

**Hyprland rule for a floating player**, if you want the "floating music
widget" look:

```conf
windowrulev2 = float, class:^(mfe-player)$
windowrulev2 = size 900 600, class:^(mfe-player)$
windowrulev2 = center, class:^(mfe-player)$
```

Blur comes from Hyprland's global `decoration:blur` and the terminal's own
opacity, not a per-window rule — `layerrule = blur` only applies to
layer-shell surfaces such as waybar.

Launch it with `exec-once = kitty --class mfe-player mfe` (or Ghostty, foot,
WezTerm).

---

## 🍎 macOS / yabai / AeroSpace / sketchybar

```bash
brew install cava
```

`cava`'s CoreAudio backend is the default and, on **macOS 14.2+**, `source =
tap` captures system output with no loopback app. MFE writes that config
automatically and falls back to `auto` on older systems. Grant the audio
capture permission when macOS asks.

**Transparent terminal.** Ghostty or kitty with the same `background-opacity`
settings as above; iTerm2: *Settings → Profiles → Window → Transparency*.

**sketchybar item.**

```bash
#!/usr/bin/env bash
# ~/.config/sketchybar/plugins/mfe.sh
payload="$(mfe status --json)"
text="$(printf '%s' "$payload" | /usr/bin/python3 -c 'import json,sys;print(json.load(sys.stdin)["text"])')"
sketchybar --set "$NAME" label="$text"
```

```bash
sketchybar --add item mfe right \
  --set mfe update_freq=1 script="$HOME/.config/sketchybar/plugins/mfe.sh" \
  --subscribe mfe system_woke
```

For a bar spectrum, call `contrib/mfe-spectrum.sh` instead and use its `text`
field (sketchybar does not speak waybar JSON, so pipe it through `python3 -c
'import json,sys;print(json.load(sys.stdin)["text"])'`).

**AeroSpace / yabai.** MFE is a terminal app, so it inherits your terminal's
floating rules. AeroSpace example (`~/.aerospace.toml`):

```toml
[[on-window-detected]]
if.app-id = 'com.mitchellh.ghostty'
run = 'layout floating'
```

Use `skhd`/`AeroSpace` to bind a key to `mfe toggle` and the player becomes a
global transport control for the whole desktop.

**Theme matching.** macOS Terminal/iTerm2 users can still use the built-in
palettes — they are truecolor RGB, so they render identically. Wallpaper
sampling understands the macOS desktop picture:

```bash
mfe theme adopt
```

---

## 🪟 Windows / GlazeWM / Komorebi / YASB / Rainmeter

```powershell
winget install karlstav.cava
winget install mpv
npm install -g mfe-player
```

`cava` captures the default output device automatically (WASAPI); MFE writes no
`[input]` section on Windows for exactly that reason. Use a recent cava
(0.10.4+); older winget builds can reject 5.1/7.1 devices.

**Transparent terminal.** Windows Terminal `settings.json` — `opacity` goes on
the profile, acrylic is a top-level setting:

```jsonc
{
  "profiles": {
    "defaults": {
      "opacity": 82
    }
  },
  "useAcrylic": true,
  "acrylicOpacity": 0.82
}
```

Add Mica/acrylic for the whole window with TranslucentTB if you run it.

**Windows rice stack that actually themes together:**

| Piece | Role |
|---|---|
| **GlazeWM** / **Komorebi** | tiling WM |
| **YASB** | status bar (has a native `CavaWidget`, plus custom widgets for JSON) |
| **Rainmeter** | desktop widgets |
| **Windows Terminal** | the terminal (acrylic + a Catppuccin/Gruvbox scheme) |
| **MFE** | now playing + spectrum, from the same palette |

YASB custom widget (YAML):

```yaml
mfe:
  type: "yasb.custom.CustomWidget"
  options:
    label: "<span>{data}</span>"
    label_alt: "{data}"
    class_name: "mfe"
    exec_options:
      run_cmd: "mfe status --json"
      run_interval: 1000
      return_format: "json"
    callbacks:
      on_left: "exec mfe toggle"
      on_right: "exec mfe next"
      on_middle: "exec mfe pause"
```

Map `text` from the JSON into the label (or use `contrib/mfe-spectrum.sh`
through Git Bash / WSL for the mini spectrum). YASB's own `CavaWidget` is
independent of MFE and captures the same WASAPI stream; pick one to avoid two
capture processes.

**Theme matching.** Set the terminal scheme to the same name you put in
`config.toml` (Catppuccin Mocha, Everforest, One Dark, Tokyo Night…). MFE's
palettes are the canonical hex values, so the terminal, bar and player line up
without a matugen step. If you do run matugen/wallust, generate a Windows
Terminal scheme from the same wallpaper and choose `theme = "wallpaper"` in
MFE — both sides then derive from one image.

---

## 🎛 Theme-from-wallpaper details

`theme = "wallpaper"` probes the desktop in this order:

- **Linux:** swww, hyprpaper, GNOME, KDE Plasma
- **macOS:** the current desktop picture
- **Windows:** the registry wallpaper path

Point it at a file directly if you use something exotic:

```toml
theme = "wallpaper"
wallpaper = "/home/you/Pictures/rice.png"
```

Refresh after changing the wallpaper:

```bash
mfe theme adopt
```

Or just cycle with `Alt+T` until the wallpaper palette comes up.

---

## 🔩 Status JSON contract

```jsonc
{
  "text": "▶ Title — Artist",
  "alt": "playing",              // playing | paused | stopped
  "class": "playing",
  "title": "Title",
  "artist": "Artist",
  "album": "Album",
  "position": 42.1,
  "duration": 213.0,
  "paused": false,
  "playing": true,
  "repeat": "off",               // off | all | one
  "shuffle": false,
  "volume": 80,
  "mode": "daemon",              // daemon | session | stopped
  "connected": true,
  "spectrum": [0.0, 0.12, …],    // 64 mel-spaced bands, 0..1.25
  "pulse": 0.0,                  // beat glow 0..1
  "energy": 0.37                 // overall loudness 0..1
}
```

`spectrum` is refreshed about seven times a second from the player's runtime
file, and is an empty array when nothing is playing. It is safe to poll once a
second from a bar.
