# Themes

> Part of the [MFE Player](../README.md) manual.

14 built-in themes, live cycling, and wallpaper-derived palettes.

---

## 🎨 Themes

`Alt+T` cycles live and writes the choice back to `config.toml` **without deleting your comments**.

`mfe` · `gruvbox` · `nord` · `mono` · `matrix` · `rose` · `catppuccin` · `dracula` · `tokyo-night` · `solarized` · `amber` · `ocean`

Set one permanently:

```toml
theme = "tokyo-night"
```

No theme fits? Copy `src/theme.rs`, add 13 colours, open a PR. It's genuinely that small. Code contributions that make the terminal look less like 1998 are extremely welcome.

**Blending with your wallpaper.** Two separate problems, two switches:

1. **Stop covering it.** `Alt+W` (or `transparent = true`) leaves the base background unpainted, so whatever transparency or blur your terminal already has — and therefore your wallpaper — shows through. Panels and popups stay solid, so text stays readable even over a busy photo.
2. **Match its colours.** `theme = "wallpaper"` reads the wallpaper your compositor is actually using — swww, hyprpaper, GNOME, Plasma, macOS or Windows — and derives the panel and accent colours from it. Dark wallpaper, dark panels. Warm sunset, warm accent. Or point it at a specific file with `wallpaper = "/path/to/image.jpg"`, or run `mfe theme adopt` once.

```bash
mfe theme adopt          # sample the current wallpaper and save the choice
mfe theme list           # every built-in theme, including `wallpaper`
mfe theme                # what am I using right now?
```

Your terminal still does the actual blurring — MFE just stops painting over it. kitty, wezterm, alacritty, foot, ghostty and Konsole all support opacity. On a terminal without transparency, use the palette half: it still matches the colours.

### The rest of the look

- **Rounded borders** on every panel, popup and dialog — one border type, applied everywhere.
- **Truecolor gradient wordmark** in the header and an eighth-block progress bar (`▏▎▍▌▋▊▉`) with a gradient fill and a smooth head, instead of a flat gauge.
- **Solid chrome, transparent base.** `Alt+W` removes only the base background; lists, popups and the inspector keep their own panel colour, so text survives a busy photo.
- **Wallpaper palette** keeps the wallpaper's hue in the dark panels, pulls a vivid accent from its dominant colour and rotates the hue for a second accent. Green, yellow and red stay semantic so status still means something.
- **Graceful fallbacks.** No truecolor? The palettes approximate. No wallpaper detected? The palette half is skipped instead of guessing.

---
