# Contributing to MFE Player

You found a bug at 3 a.m., fixed it, and want to send it back. Good. Here's how
to make that painless for both of us.

## Before you open a PR

```bash
cargo fmt
cargo build --release
cargo test
```

If you touched the UI, run it in a real terminal and include a screenshot or a
short recording in the PR. Terminal layouts are the easiest thing to break and
the hardest thing to explain in prose.

## What makes a great PR

- **One feature or fix per PR.** Big rewrites are hard to review and easy to
  reject. Small, sharp changes land fast.
- **Keep the hotkeys documented.** If a key does something new, add it to the
  help overlay (`?`), the command palette, and the README keymap. An
  undocumented hotkey is a secret handshake nobody asked for.
- **Respect the plugin contract.** Metadata, streaming and lyrics are separate
  Python plugins using only the standard library. No new pip dependencies in
  core.
- **Don't break the daemon.** Background playback must survive TUI exit,
  repeat/shuffle, queue advancement and reattach.
- **Config comments are sacred.** Any config rewrite must preserve user
  comments.

## Reporting a bug

Include:

1. `mfe status` output
2. `mpv --version` and your distro
3. What you pressed, what you expected, what happened
4. The last lines of `~/.config/mfe-player/mpv.log` if playback is involved

## Adding a theme

Themes are 13 colours in `src/theme.rs`. Add the constant, add it to
`THEME_NAMES` and the `preset()` match, update the README list, done.

## Adding a plugin

Drop a `.py` file in `~/.config/mfe-player/plugins/` first and confirm it works
before proposing it as a bundled default. Bundled plugins live in `plugins/`
and are embedded at compile time.

## Code of conduct

Be decent. We're here because subscriptions suck, not because we want a second
job moderating arguments about tabs versus spaces.

By contributing you agree your work is licensed under the MIT licence.
