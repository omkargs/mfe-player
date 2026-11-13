# Install

> Part of the [MFE Player](../README.md) manual.

Every install channel below is built from the same tagged commit.

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

