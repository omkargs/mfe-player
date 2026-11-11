# mfe-player — cross-platform, social and taste-aware roadmap

Status: draft · Owner: omkar · Last updated: 2026-11-11

This is the working plan for the next major version. It covers:

1. Making Windows a real, tested target (not an experimental CI job).
2. A Spotify-style autoplay + taste engine ("more phonk if you listen to phonk").
3. Discord Rich Presence so friends can see what you are playing.
4. Shipping one Rust codebase to Linux, macOS and Windows through npm.
5. Per-OS media integration (MPRIS / SMTC / Now Playing).
6. A backlog of smaller features, risks and open decisions.

Non-goals for this cycle: a separate Windows fork, bundling mpv/yt-dlp, cloud
accounts, and true listen-along audio rooms (separate project).

---

## 1. Current state audit

| Area | What exists | What is missing / broken |
|---|---|---|
| npm platform detection | `npm/mfe-player/bin/mfe.js` maps `linux`/`darwin`/`win32`, `x64`/`arm64`, glibc vs musl, and picks the optional package. | Only `mfe-player-linux-x64@0.2.0` has a real binary. mac/win/arm packages are empty placeholders. No `win32-arm64`. No fallback when optional dependencies are skipped. |
| Release pipeline | `.github/workflows/release.yml` builds 6 targets and publishes npm/PyPI/crates. | Windows CI is `experimental: true` (allowed to fail). No checksums or fallback manifest. No signing/notarisation. |
| Windows compile | `src/ipc.rs` already abstracts Unix sockets vs named pipes for mpv; config has `APPDATA`/`LOCALAPPDATA` branches. | `mpris-server` is an unconditional dependency and `mod mpris` is unconditional. Windows will not build cleanly today. `taskkill`, `Path::exists()` on pipes, `fs::rename` over an existing file are wrong on Windows. |
| Daemon | Headless player owns mpv, advances queue, serves MPRIS, persists state. | No graceful control channel (uses `taskkill`). PID reuse not handled. No cross-platform lock. Session ownership is pid-file based. |
| Radio / autoplay | `radio` toggle; when the last queued track starts it searches the current artist/title via metadata plugins and appends results. | No taste model, no skip feedback, no genre data, no diversify/probe logic, no persistence across sessions, only primes one track ahead. |
| History | Last 500 plays with `played_at`. | No completion ratio, skip events, source/context, or per-track outcome. |
| Metadata | iTunes, MusicBrainz, Internet Archive, Radio Browser, SoundCloud/YouTube streaming. | `Track` has no `genre`/`tags` fields, so genre is thrown away even when providers return it. No `similar` capability. |
| Scrobbling | ListenBrainz submission with token. | No Last.fm, no offline retry queue, no "now playing" update. |
| Media | MPRIS on Linux, media keys via playerctl, notifications per OS, clipboard share, wallpaper palettes. | No SMTC (Windows), no Now Playing (macOS), no Windows toast. |
| UI features | Transparency, ambient art, full-screen now playing, ASCII icons, responsive chrome, EQ, ReplayGain, fades, cava, doctor, update. | — |

---

## 2. Principles

- **One codebase.** Platform differences live behind `cfg` and a small platform
  layer. A separate Windows fork would double every future bug.
- **Local-first.** Taste profiles, history and presence data stay on the machine.
  No new service, no account, no telemetry.
- **Graceful degradation.** Missing mpv, Discord, cava, notifier or media API must
  never break playback or setup.
- **Explainable recommendations.** We start with a weighted tag/artist model, not
  an opaque neural net, so `mfe taste` can say *why* a track was chosen.
- **Feedback must not eat itself.** Autoplay picks are down-weighted as evidence
  so the player cannot lock itself into a loop.

---

## 3. Phase 0 — platform foundation (3–4 days)

Everything else depends on this.

### 3.1 Target-gate Linux-only code

```toml
[target.'cfg(target_os = "linux")'.dependencies]
mpris-server = "0.11"
```

- `mod mpris` and the `mpris` setting become Linux-only, behind a no-op `Media`
  trait on other OSes.
- Gate cava (Linux binary), wallpaper probing, and Linux clipboard tools.

### 3.2 New `src/platform/` layer

```
src/platform/mod.rs      traits: Media, Notify, Clipboard, Wallpaper, Paths, Process
src/platform/linux.rs    MPRIS + notify-send + wl-copy/xclip
src/platform/macos.rs    MPNowPlayingInfoCenter + osascript + pbcopy
src/platform/windows.rs  SMTC + toast + clip.exe + SPI_GETDESKWALLPAPER
```

### 3.3 Windows correctness fixes

- Named pipes: `Path::exists()` is always false for `\\.\pipe\...`. Replace
  existence probes with connect attempts in `audio.rs` and the daemon.
- `fs::rename` over an existing file fails on Windows. Add `replace_file()`
  (remove-then-rename or `ReplaceFileW`) for `state.json` and config writes.
- Daemon stop: add a control pipe (`\\.\pipe\mfe-player-ctl-<user>`) that accepts
  `{"cmd":"stop"}`, saves state and exits. Keep `taskkill /F` as last resort only.
- mpv spawn: `CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP`, never
  `DETACHED_PROCESS` (console flashes and broken stdio).
- Process lookup: use a `which`-style helper (`mpv`, `mpv.exe`, `yt-dlp`,
  `yt-dlp.exe`, `python`, `python3`), plus `%LOCALAPPDATA%\mfe-player\bin` for
  portable installs.
- Paths: NUL instead of `/dev/null`, `USERNAME` fallback, drive-letter safe
  argument quoting for `--input-ipc-server=`.
- Session/daemon pid files: store `pid + process start time` to survive PID reuse.

### 3.4 Honest CI

- Remove `experimental: true` for Windows.
- Matrix: `cargo check` for all 6 targets, `cargo test` on Linux/macOS/Windows,
  then smoke `mfe --version` and `mfe doctor`.
- Add `cargo fmt --check` + `clippy -D warnings` (at least on Linux).

**Acceptance:** green CI on Linux, macOS and Windows; Windows binary launches,
opens mpv over a named pipe, plays a local file, and stops gracefully.

---

## 4. Phase 1 — taste engine + autoplay (6–8 days)

The headline feature: when a queue or playlist ends and repeat is off, the
player keeps going with something that fits, and it learns from what you skip
and what you finish.

### 4.1 Current radio, and why it is not enough

`App::start_radio()` asks the metadata plugin for `artist` → `title` → `title +
artist` and appends the first non-empty result. It is stateless, ignores skips,
has no genre input, and can return the same cluster forever. Autoplay must be a
scoring problem, not a single search.

### 4.2 Signals

Capture at track boundaries in both the TUI (`app.rs`) and daemon
(`daemon.rs`). A track session starts on `start_stream` and ends on
stop/skip/EOF/error.

| Event | Condition | Weight |
|---|---|---|
| like / favorite | explicit | **+3.0** |
| repeat | same track restarted, or repeat-one | **+2.0** |
| completion | ≥ 90% or ≥ 30 s of a short track | **+1.0** |
| half listen | 50–90% | +0.5 |
| quick skip | < 15 s | **−2.0** |
| early skip | 15–40 s | −1.0 |
| late skip | 40–70% | −0.3 |
| manual queue add / click result | explicit intent | +0.8 |
| dislike (`X` action) | explicit | **−4.0**, item blocked 30 days |

Context multiplier: tracks that were auto-selected count **0.6×** so the model
does not reinforce its own guesses; explicit likes/completions count fully.

### 4.3 Profile model

Two levels, persisted in `~/.config/mfe-player/taste.json` (override with
`MFE_TASTE_DIR` for tests):

```json
{
  "version": 1,
  "updated_at": 1762800000,
  "long_term": {
    "tags":    { "phonk": 14.2, "drift phonk": 6.1, "jazz": 2.0 },
    "artists": { "artist-id": 8.4 },
    "sources": { "youtube": 5.0 },
    "years":   { "2020s": 3.0 }
  },
  "session": {
    "id": "2026-11-11T15:00:00Z",
    "started_at": 1762800000,
    "tags": { "phonk": 3.0 },
    "artists": {},
    "plays": 7,
    "skips": 2,
    "completed": 4,
    "confidence": 0.62
  },
  "events": [
    { "id": "...", "tags": ["phonk"], "artist": "...", "outcome": "complete",
      "fraction": 0.97, "auto": true, "at": 1762800123 }
  ],
  "probes": { "last_at": 0, "trials": 3, "successes": 2 }
}
```

- `events` is capped at 2000 entries (ring buffer). Aggregates are updated on
  write, so scoring never scans the full log.
- Decay: long-term half-life **30 days**, session half-life **90 minutes**.
  Session resets after 30 minutes idle or `mfe stop`; it survives Ctrl+B and
  daemon handoff because it is keyed by session id, not process.
- `confidence` = smoothed completion rate over at least 5 plays. Probes require
  confidence ≥ 0.55.

### 4.4 Tag derivation

`Track` gains optional `genre: Vec<String>` and `tags: Vec<String>` (serde
default, so plugins can supply them without breaking older payloads).

Sources, in order:

1. Plugin-provided `genre`/`tags` (iTunes `primaryGenreName`, MusicBrainz tags,
   Radio Browser tags, SoundCloud genre).
2. A normaliser/lexicon in `src/taste.rs`:
   `drift phonk`, `brazilian phonk`, `house phonk` → `phonk`;
   `slowed + reverb` stays separate; `lo-fi`, `lofi`, `chillhop` → `lofi`.
3. Title/channel tokens as weak tags (`mix`, `remix`, `sped up`, `nightdrive`).
4. Source and era as coarse tags.

No audio analysis in v1. A later optional step can compute BPM/energy locally
with essentia/aubio, but it is not on the critical path.

### 4.5 Candidate generation

When the queue is within 2 tracks of the end (or empty), build ~30 candidates:

| Source | Query | Notes |
|---|---|---|
| Same artist | plugin `similar` / `search <artist>` | cap to 1 per artist per 3 picks |
| Similar tags | `search "<top tag> mix"`, `"<tag> <decade>"` | e.g. `phonk mix`, `drift phonk 2024` |
| Co-occurring artists | artists from sessions that share tags | local graph, no network |
| ListenBrainz recommendations | `/1/recommendations/user/{user}/...` | only when a token exists (already supported) |
| Current radio | existing artist/title search | fallback and diversity source |

Candidates are deduplicated by id, cached for 24 h in
`~/.cache/mfe-player/rec-cache.json`, and filtered:

- no repeat of the last 20 tracks or the current queue,
- no blocked items unless older than 30 days,
- drop preview-only streams (`short_preview`),
- drop tracks that failed to resolve recently.

### 4.6 Scoring

```
score(c) = 0.35 · tagAffinity(c)
         + 0.25 · artistAffinity(c)
         + 0.20 · sessionAffinity(c)
         + 0.10 · novelty(c)
         + 0.05 · external(c)          # ListenBrainz / plugin similar
         + 0.05 · explorationBonus(c)
```

- `tagAffinity` is cosine similarity between the candidate's tag vector and the
  decayed profile (TF-IDF weighted so `phonk` beats `music`).
- `artistAffinity` = artist weight + Jaccard of co-played artists.
- `sessionAffinity` uses the session profile only, so a phonk-heavy evening
  dominates without immediately rewriting 30-day taste.
- `novelty` penalises artists/tags played in the last hour.
- `explorationBonus` comes from an epsilon-greedy arm per top-level tag
  (ε = 0.12 by default), or Thompson sampling once enough feedback exists.

Selection: **MMR** (maximal marginal relevance) with λ = 0.7 to keep the batch
varied, then a hard cap of one track per artist per 3 picks and no more than 60%
of a batch from a single tag.

### 4.7 The "mismatch test" probe

The user asked for a deliberate odd track to measure preference. Implement it
as an exploration slot, with guardrails:

- Eligible only when the session has ≥ 8 plays and confidence ≥ 0.55.
- At most **1 probe per 10 autoplay picks**, never two in a row, never in the
  first 5 tracks, never while user-queued tracks are waiting.
- Pick from the *adjacent* tag cluster (second-best tag, or a tag that
  co-occurs 10–30% of the time), not a random genre.
- Mark it `probe: true` in the event log and show a subtle `· trying something
  new` note in the status line (toggleable with `autoplay_probe_hint`).
- Evaluate after the track: completion/like widens that neighbour's weight by
  +0.15; a quick skip narrows it by −0.15. Two failed probes of the same tag
  stop probing it for 7 days.
- `mfe taste` shows probe history and success rate; `mfe taste reset probes`
  clears it.

### 4.8 Skip logic ("if they skip, they only want phonk this session")

Skips are attributed to the *tags and artist of the skipped track*, not to the
tag of the previous one:

- Skipping a jazz track during a phonk session decreases `jazz` and increases
  the relative weight of the session's dominant tags.
- Five quick skips of phonk in a row must drop phonk's share of the next batch
  below 30% — even if 30-day taste is phonk-heavy. Test this explicitly.
- Skips of user-queued tracks count less (0.5×) than skips of autoplay picks,
  because the user may simply be rearranging.

### 4.9 Autoplay behaviour

New config (all serde-defaulted so old configs keep working):

```toml
# off | similar | taste
autoplay = "taste"
autoplay_explore = 0.12
autoplay_probe_every = 10
autoplay_probe = true
taste_enabled = true
```

Rules:

- Trigger when the queue ends and `repeat != all`, unless "stop after current"
  is active or the user pressed stop/quit this session.
- Prefetch when 2 tracks remain, so the transition is gapless (today it primes
  at 1 and resolves after the track ends).
- If generation fails: exponential backoff, then artist radio, then stop with
  `autoplay: could not find more — press r to retry`.
- `radio` stays as a manual/legacy mode; autoplay reuses its search as one
  candidate source.
- Works identically when the daemon owns playback: the engine lives in `App`
  logic shared by TUI and daemon, and taste persistence happens in both.
- `mfe autoplay off|similar|taste` and a settings row; changing it mid-session
  takes effect at the next boundary.

### 4.10 Persistence and privacy

- `taste.json` is local, human-readable, and never uploaded. ListenBrainz stays
  opt-in and separate.
- Writes are debounced (30 s) and atomic (`taste.json.tmp` + replace).
- `mfe taste` — top tags/artists, confidence, probe stats.
- `mfe taste explain` — why the last pick was chosen (score breakdown).
- `mfe taste reset [session|all|probes]`.
- Export/delete in `mfe cache` style, and `taste.json` is excluded from share.

### 4.11 Files touched

```
src/taste.rs          new: profile, decay, events, tag lexicon, scoring
src/recommend.rs      new: candidate generation, MMR, cache
src/models.rs         Track.genre / Track.tags
src/app.rs            outcome tracking, autoplay trigger, prefetch, keys
src/daemon.rs         same engine + persistence when headless
src/config.rs         autoplay/taste keys + docs template
src/main.rs           `mfe taste ...` subcommands
plugins/metadata_*.py emit genre/tags; add optional `similar` command
```

### 4.12 Tests

Unit:

- decay halves at 30 days / 90 minutes,
- outcome classification at 14 s, 39 s, 60%, 95%,
- tag normalisation (`drift phonk` → `phonk`),
- MMR diversity and artist caps,
- probe gating (confidence, spacing, no back-to-back),
- skip burst lowers a tag's batch share below 30%.

Simulation harness (no network):

- synthetic "phonk-only" user → ≥ 60% phonk in batches after warm-up,
- "phonk then jazz" session → jazz grows within 5 completions,
- "skips everything" user → autoplay stops suggesting (or broadens) instead of
  collapsing into one tag,
- cold start with empty taste → falls back to artist radio, no panic.

Offline evaluation (optional, `#[ignore]`): hold out the last 20% of history,
measure hit@5 against the profile built from the rest.

**Acceptance:** with repeat off, ending a queue continues playback within one
track gap; five phonk completions raise phonk share above 60%; five quick phonk
skips drop it below 30%; probes appear at most once per 10 picks; `mfe taste
reset` returns to cold start; Ctrl+B preserves the session.

---

## 5. Phase 2 — Discord Rich Presence (3–5 days)

Friends see *what* you are playing, not the audio.

### 5.1 Transport

- Linux/macOS: `$XDG_RUNTIME_DIR/discord-ipc-0..9` (fallback `$TMPDIR`,
  `/tmp`; also try `app/com.discordapp.Discord/discord-ipc-0` for Flatpak/Snap).
- Windows: `\\.\pipe\discord-ipc-0..9`.
- Frame: 4-byte LE opcode + 4-byte LE length + JSON. Op 0 handshake
  `{"v":1,"client_id":"..."}`, then op 1 `SET_ACTIVITY`.
- Reconnect with backoff; idle silently when Discord is not running.

### 5.2 Activity

- `details` = title (≤128), `state` = artist — album.
- `timestamps.start/end` so Discord renders progress; update only on
  play/pause/track change/seek (rate limit ≈ 5 updates / 20 s).
- Assets: `mfe` large image, play/pause small image, optional source icon.
- Buttons (max 2): "Get mfe-player" → repo, "See my taste" → repo docs.
- Activity type: Listening.

### 5.3 Lifecycle

- Runs **inside the daemon** when one exists, otherwise in the TUI, fed by the
  same `NowPlaying` snapshot MPRIS already builds.
- One owner only; handoff when the daemon starts/stops.
- Never blocks playback; a dead socket is a no-op.

### 5.4 Privacy and config

```toml
discord = false                 # opt-in
discord_client_id = ""          # built-in default if empty
discord_show_paused = false
discord_show_buttons = true
discord_when_private = false    # hide in private/headless mode
```

`mfe discord on|off|status`. First enable prints a one-line note about what is
shared. No local file paths, no tokens, no IPs.

### 5.5 Required from the owner

- Create a Discord application, provide `client_id`.
- Upload assets: `mfe`, `play`, `pause`, and three source icons.

**Acceptance:** Discord desktop (stable, PTB, Canary) on Linux and Windows shows
the track; closing Discord and reopening it recovers; disable hides it
immediately; no updates while paused unless enabled.

---

## 6. Phase 3 — ship all platforms (5–7 days)

### 6.1 npm packages

Add `mfe-player-win32-arm64` (and optionally `linux-armv7`). Every package
ships exactly one binary (`bin/mfe-player` or `bin/mfe-player.exe`) and has
correct `os`/`cpu`/`libc` fields.

Launcher (`bin/mfe.js`) upgrades:

- Alpine detection via `/etc/alpine-release` in addition to
  `process.report`.
- Rosetta: prefer `darwin-arm64` when running under `process.arch === "x64"`
  on Apple Silicon.
- Fallback when the optional package is missing: download the GitHub release
  tarball for the detected target into `$XDG_CACHE_HOME/mfe-player/bin`, verify
  SHA-256 from a `checksums.txt` / `latest.json`, then exec. Respect
  `HTTPS_PROXY` and `NO_PROXY`.
- Clear per-OS guidance instead of a Node stack trace.
- `engines: { node: ">=18" }`, and Windows `.exe` naming throughout.

### 6.2 Publish order

`npm/scripts/set-version.mjs` bumps launcher + platform packages together.
Publish order must be: platform packages → launcher. Add a `prepublish` check
that every platform package contains its binary, is non-empty, and has the
executable bit (Unix). A failed platform publish must abort the release.

### 6.3 Windows installer and first run

- `install.ps1`: detects mpv/yt-dlp/python/ffmpeg, offers `winget` or portable
  download into `%LOCALAPPDATA%\mfe-player\bin`, adds that dir to the user PATH,
  then runs `mfe doctor`.
- npm launcher first run calls the same setup logic on Windows (PowerShell)
  instead of bash, using a marker in `%LOCALAPPDATA%\mfe-player\npm-setup.done`.
- Shell completions: PowerShell instead of bash/zsh/fish; skip when the binary
  lacks `completions` (same `--help` probe as today).
- `mfe doctor` reports per-OS hints (winget, brew, apt/pacman).

### 6.4 Signing

- macOS: unsigned binaries are blocked by Gatekeeper. Either buy an Apple
  Developer account ($99/yr) and notarise, or document
  `xattr -d com.apple.quarantine`.
- Windows: Authenticode certificate (~$100–400/yr) removes SmartScreen warnings;
  otherwise document "More info → Run anyway".
- Decision required before calling macOS/Windows "just works".

### 6.5 Release artefacts

Per target: tarball/zip, `sha256`, and a shared `latest.json`:

```json
{ "version": "0.3.0",
  "targets": { "x86_64-unknown-linux-gnu": { "url": "...", "sha256": "..." } } }
```

One tag drives GitHub Release, npm (all packages), PyPI and crates.

**Acceptance:** `npm i -g mfe-player` on Windows, macOS Intel/ARM and Linux
glibc/musl installs and runs; `mfe --version` prints the release version on
every platform; a missing optional package triggers the signed download
fallback.

---

## 7. Phase 4 — OS media integration (4–6 days)

| OS | Now playing / media keys | Notifications | Clipboard |
|---|---|---|---|
| Linux | MPRIS (done) | notify-send (done) | wl-copy/xclip (done) |
| Windows | SMTC via `windows` crate: media keys, volume flyout, lock screen | Windows toast | `clip.exe` |
| macOS | `MPNowPlayingInfoCenter` + `MPRemoteCommandCenter` via `objc2` | osascript (done) | pbcopy (done) |

- One `Media` trait, three implementations.
- Artwork forwarding where the platform supports it (SMTC thumbnails, macOS
  artwork) from the existing cover cache.
- `mpris = true` becomes `media = "auto" | "off"` (keep `mpris` as an alias).
- Fallback: if SMTC/Now Playing fails, playback continues silently.

**Acceptance:** hardware media keys work on all three OSes; the OS panel shows
title/artist/artwork; no crash when the API is unavailable.

---

## 8. Phase 5 — backlog (pick in order)

| # | Feature | Effort | Notes |
|---|---|---|---|
| 1 | Last.fm scrobbling + offline queue | 2 d | Reuse ListenBrainz plumbing; retry on reconnect; "now playing" ping. |
| 2 | Share card PNG | 2 d | `Y` gains `--card`: cover + title + album + QR, copied or saved. |
| 3 | `mfe serve` phone remote | 4 d | Localhost HTTP + bearer token + QR; play/pause/queue/search. No cloud. |
| 4 | Playlist import/export | 3 d | m3u/json/csv; import Spotify/YouTube/Apple URLs via yt-dlp metadata. |
| 5 | Gapless + crossfade | 2 d | `--gapless-audio`, fade config already exists. |
| 6 | Sleep timer / alarm / per-device volume | 2 d | Timer survives daemon handoff; remembers volume per `audio_device`. |
| 7 | Plugin store v2 | 4 d | Registry index, sha256 manifests, permissions, auto-update. |
| 8 | Library upgrades | 3 d | Watch folders, duplicate detection, tag editing, album/artist views. |
| 9 | Listen-along rooms | weeks | Separate product: audio relay + WebRTC/HTTP; out of scope now. |

---

## 9. Daemon and portability risks

| Risk | Mitigation |
|---|---|
| `mpris-server` blocks Windows/macOS builds | Target-gate it; `Media` trait with no-op implementations. |
| Named-pipe `Path::exists()` lies | Connect-probe helper; never rely on metadata for pipes. |
| `fs::rename` over existing file fails on Windows | `replace_file()` helper for state/config/taste. |
| `taskkill` kills without saving | Control pipe with graceful stop; `/F` only as last resort. |
| PID reuse | Store pid + start time, verify both. |
| Two instances racing on state | Cross-platform advisory file lock (`fs2`/`fd-lock`). |
| macOS socket path 104-byte limit | Short hashed socket path in `$TMPDIR`. |
| Windows console flashes | `CREATE_NO_WINDOW`, detached process group. |
| mpv argument quoting with backslashes/spaces | `OsStr` args, never string concatenation. |
| cava/ffmpeg/yt-dlp missing | `doctor` hints, optional features, no hard dependency. |
| Daemon vs TUI ownership | Single taste session id + control channel + atomic handoff. |

---

## 10. Testing and release

- CI matrix: Linux glibc/musl/arm64, macOS x64/arm64, Windows x64 (arm64 when
  a runner exists).
- Unit tests for taste/decay/MMR/probe logic; goldens for plugin payloads.
- Simulation harness with synthetic users (see 4.12).
- Smoke tests per OS: `--version`, `doctor`, start/stop daemon, play a bundled
  silent audio fixture through mpv, verify state save.
- Release: one tag → binaries + checksums + `latest.json` → npm platform
  packages → npm launcher → PyPI → crates → GitHub release.
- Versioning: `0.3.0` for the cross-platform + autoplay release; npm launcher and
  platform packages always share a version from then on.

---

## 11. Open decisions

1. **Discord app id and assets** — needs the owner's Discord Developer Portal
   account. Blocking for pretty presence, not for the protocol work.
2. **Where do Windows/macOS builds run?** GitHub Actions is the only realistic
   route; that requires pushing the source (private repo or a `builds` branch).
   Local-only means no mac builds (needs Apple SDK/hardware) and untested
   Windows.
3. **Windows dependency install** — winget (may prompt UAC) vs portable
   downloads into `%LOCALAPPDATA%` (no admin, more code).
4. **Signing budget** — Apple $99/yr, Windows cert ~$100–400/yr, or ship
   unsigned with documented workarounds.
5. **Autoplay default** — plan says `autoplay = "taste"` (on) so playlists
   never stop; `radio` stays opt-in. Confirm.
6. **Probe aggressiveness** — default 1 per 10 picks; make it visible in the
   status line or silent?
7. **First backlog item after Discord** — Last.fm, share cards, or phone remote.

---

## 12. Milestones

| Milestone | Content | Estimate | Status |
|---|---|---|---|
| M0 | Platform layer + green CI on 3 OS | 3–4 d | **foundation landed locally · target gating + platform layer + Windows fixes · CI matrix updated, needs a run** |
| M1 | Taste engine + autoplay + probes + `mfe taste` | 6–8 d | **implemented locally · 30/30 tests green · pending TUI check + commit** |
| M2 | Discord presence | 3–5 d | blocked on owner Discord app ID/assets |
| M3 | npm packages for all targets + Windows installer + signing decision | 5–7 d | not started |
| M4 | SMTC/Now Playing + notifications | 4–6 d | not started |
| M5 | Backlog picks | ongoing | — |

### 12.1 M1 implementation notes (2026-11-11)

What landed differs slightly from the sketch above:

* **`known_tags` provenance.** `taste.json` records which tags came from genre
  metadata or plugin tags. Session and long-term weights still include title
  words for similarity, but only `known_tags` are eligible as mismatch probes —
  otherwise a track called "Midnight Phonk Mix" would make the player probe the
  genre "midnight".
* **Vetoes are decisive.** When a session tag falls to `≤ -2.0`, candidates
  carrying it take an extra `-1.2 × tag weight` in scoring. MMR diversity alone
  could otherwise re-insert a genre the listener just skipped five times.
* **`mfe taste`.** `status` (default), `explain`, `reset [session|all|probes]`,
  `like`, `dislike`. `dislike` blocks the current track for 30 days.
* **Metadata.** iTunes emits `genre`, Radio Browser emits station `tags`,
  MusicBrainz passes through genres/tags when present, and local files read
  `genre` from their embedded tags.
* **Previous restarts are not skips.** Pressing previous at index 0 just seeks
  or restarts; only real track changes feed the profile.
* **Autoplay fallbacks mark `auto_queued`** so completions of fallback tracks
  count as autoplay picks, not deliberate listening.

### 12.2 M0 progress (2026-11-11)

* `mpris-server` and `libc` are now platform-scoped dependencies; the Linux MPRIS
  module is only compiled on Linux.
* New `src/platform/` layer: `media` (MPRIS on Linux, inert stub elsewhere) and
  `fs_util::replace_file` for atomic writes that work when Windows refuses to
  rename over an existing file.
* Windows IPC fixes: mpv is probed by connecting instead of `Path::exists()`,
  socket directories are only created on Unix, and mpv is spawned with
  `CREATE_NO_WINDOW` instead of `DETACHED_PROCESS`.
* Config, state and taste writes now go through the platform replace helper.
* Plugins run through `python` on Windows, permission bits are Unix-only, cava is
  Linux-only, and doctor reports platform-appropriate checks.
* CI matrix now includes Windows as a supported target and runs fmt + clippy.
* Still open for M0: a Windows daemon control channel (graceful stop without
  `taskkill`), pid + start-time files, and an actual CI/hardware run.

Recommended order: **M0 → M1 → M2** first. M1 is pure Rust and fully testable
locally, so it can start in parallel with M0 if desired.
