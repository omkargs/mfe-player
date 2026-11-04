# mfe-player

A plugin-driven terminal music player. This package ships the launcher and
pulls in the right prebuilt binary for your platform; there is no install
script and no download at first run.

```bash
npm install -g mfe-player     # also: bun add -g mfe-player
mfe
```

You also need **mpv** (audio), **python3** (plugins) and **yt-dlp**
(streaming). `mfe doctor` tells you what is missing and how to fix it.

Prefer a package manager? `cargo install mfe-player`, `pipx install
mfe-player`, or the install script from the repo.

MIT.
