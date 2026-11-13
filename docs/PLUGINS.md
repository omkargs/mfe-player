# Plugins are just Python

> Part of the [MFE Player](../README.md) manual.

Providers are plain Python files that print JSON. This page is the whole contract.

---

## 🔌 Plugins are just Python

Drop a `.py` file into `~/.config/mfe-player/plugins/`. It gets discovered, loaded and kept up to date on launch. Standard library only — no virtualenv, no build step, no dependency hell.

The bundled ones:

- **metadata** — iTunes Search API, MusicBrainz, Internet Archive, Radio Browser
- **streaming** — YouTube (`yt-dlp`), SoundCloud (`yt-dlp`), Internet Archive, Radio Browser
- **lyrics** — LRCLIB with a lyrics.ovh fallback

If one provider rate-limits you, MFE fails over to the next plugin instead of showing you a spinner forever. Adding a new source is a JSON contract, not a framework.

The trick, if you want to build one: your plugin prints JSON. That's it. `main.py search '<query>'` returns `{"artists":[],"albums":[],"tracks":[...]}`, and `main.py resolve '<track-json>'` returns `{"url":"...","extractor":"..."}`. If it works in a terminal, it works in MFE.

---
