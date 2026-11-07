use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub fn config_dir() -> PathBuf {
    dirs_config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mfe-player")
}

fn legacy_config_dir() -> PathBuf {
    dirs_config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("nuclear-tui")
}

fn legacy_cache_dir() -> PathBuf {
    if let Ok(xdg) = env::var("XDG_CACHE_HOME") {
        if !xdg.trim().is_empty() {
            return PathBuf::from(xdg).join("nuclear-tui");
        }
    }
    env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".cache").join("nuclear-tui"))
        .unwrap_or_else(|| PathBuf::from(".cache").join("nuclear-tui"))
}

/// Move an old nuclear-tui install over to the mfe-player paths so nobody
/// loses their queue, playlists, downloads or hand-written config in the
/// rename. Safe to call on every launch.
pub fn migrate_legacy() {
    let new = config_dir();
    let old = legacy_config_dir();
    if !new.exists() && old.is_dir() {
        if fs::rename(&old, &new).is_err() {
            let _ = copy_dir(&old, &new);
        }
    }
    let new_cache = cache_dir();
    let old_cache = legacy_cache_dir();
    if !new_cache.exists() && old_cache.is_dir() && fs::rename(&old_cache, &new_cache).is_err() {
        let _ = copy_dir(&old_cache, &new_cache);
    }
}

fn copy_dir(from: &PathBuf, to: &PathBuf) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn dirs_config_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    if let Ok(appdata) = env::var("APPDATA") {
        if !appdata.trim().is_empty() {
            return Some(PathBuf::from(appdata));
        }
    }
    if let Ok(xdg) = env::var("XDG_CONFIG_HOME") {
        if !xdg.trim().is_empty() {
            return Some(PathBuf::from(xdg));
        }
    }
    env::var_os("HOME").map(|home| PathBuf::from(home).join(".config"))
}

pub fn plugins_dir() -> PathBuf {
    config_dir().join("plugins")
}

pub fn state_path() -> PathBuf {
    config_dir().join("state.json")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn mpv_log_path() -> PathBuf {
    config_dir().join("mpv.log")
}

pub fn cache_dir() -> PathBuf {
    #[cfg(windows)]
    if let Ok(local) = env::var("LOCALAPPDATA") {
        if !local.trim().is_empty() {
            return PathBuf::from(local).join("mfe-player");
        }
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home)
            .join("Library")
            .join("Caches")
            .join("mfe-player");
    }
    if let Ok(xdg) = env::var("XDG_CACHE_HOME") {
        if !xdg.trim().is_empty() {
            return PathBuf::from(xdg).join("mfe-player");
        }
    }
    env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".cache").join("mfe-player"))
        .unwrap_or_else(|| PathBuf::from(".cache").join("mfe-player"))
}

pub fn downloads_dir() -> PathBuf {
    cache_dir().join("downloads")
}

pub fn artwork_dir() -> PathBuf {
    cache_dir().join("artwork")
}

pub fn library_cache_path() -> PathBuf {
    config_dir().join("library.json")
}

pub fn runtime_dir() -> PathBuf {
    if let Ok(xdg) = env::var("XDG_RUNTIME_DIR") {
        if !xdg.trim().is_empty() {
            return PathBuf::from(xdg).join("mfe-player");
        }
    }
    let user = env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_else(|_| "user".to_string());
    env::temp_dir().join(format!("mfe-player-{user}"))
}

#[cfg(windows)]
fn user_name() -> String {
    env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_else(|_| "user".to_string())
}

/// mpv listens on a Unix socket everywhere except Windows, where it exposes
/// a named pipe instead.
pub fn mpv_socket_path() -> PathBuf {
    #[cfg(windows)]
    {
        return PathBuf::from(format!(r"\\.\pipe\mfe-player-{}", user_name()));
    }
    #[cfg(not(windows))]
    {
        runtime_dir().join("mpv.sock")
    }
}

/// Records which TUI session owns the shared mpv instance. A second,
/// concurrently running session must not hijack it, so it gets its own
/// private socket instead.
pub fn session_lock_path() -> PathBuf {
    runtime_dir().join("session.pid")
}

/// PID of the headless background player, when one is running.
pub fn daemon_pid_path() -> PathBuf {
    runtime_dir().join("daemon.pid")
}

pub fn daemon_log_path() -> PathBuf {
    config_dir().join("daemon.log")
}

pub fn private_mpv_socket_path(pid: u32) -> PathBuf {
    #[cfg(windows)]
    {
        return PathBuf::from(format!(r"\\.\pipe\mfe-player-{}-{pid}", user_name()));
    }
    #[cfg(not(windows))]
    {
        runtime_dir().join(format!("mpv-{pid}.sock"))
    }
}

fn default_true() -> bool {
    true
}

fn default_seek() -> f64 {
    5.0
}

fn default_long_seek() -> f64 {
    30.0
}

fn default_volume_step() -> f64 {
    5.0
}

fn default_theme() -> String {
    "mfe".to_string()
}

fn default_view() -> String {
    "search".to_string()
}

fn default_cava_position() -> String {
    "right".to_string()
}

fn default_cava_bars() -> u16 {
    28
}

fn default_eq() -> String {
    "flat".to_string()
}

/// User-editable settings, loaded from `~/.config/mfe-player/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub theme: String,
    pub seek_step: f64,
    pub long_seek_step: f64,
    pub volume_step: f64,
    pub default_view: String,
    pub prefetch: bool,
    pub radio: bool,
    pub mouse: bool,
    pub cava: bool,
    pub cava_position: String,
    pub cava_bars: u16,
    pub normalize: bool,
    pub eq: String,
    /// Custom EQ curve in dB for the ten bands (only used when eq = "custom").
    pub eq_gains: Vec<f64>,
    pub audio_device: String,
    pub mpris: bool,
    /// Folders scanned for local audio (FLAC/MP3/Opus/...). `~` is expanded.
    pub library_paths: Vec<String>,
    /// ListenBrainz user token. Empty disables scrobbling entirely.
    pub listenbrainz_token: String,
    /// Seconds of fade at each end of a track (0 disables smart fade).
    pub fade_seconds: f64,
    /// ReplayGain gain applied to files that carry the tags: off | track | album.
    pub replaygain: String,
    /// Leave the base background unpainted so the terminal's transparency
    /// (and therefore the wallpaper) shows through.
    pub transparent: bool,
    /// Explicit wallpaper for `theme = "wallpaper"`. Empty = ask the desktop.
    pub wallpaper: String,
    /// Shift the accent colours to the dominant colour of the current cover.
    pub ambient_art: bool,
    /// Glyph set: nerd (default), ascii, or auto.
    pub icons: String,
    /// Desktop notification on track change.
    pub notifications: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            seek_step: default_seek(),
            long_seek_step: default_long_seek(),
            volume_step: default_volume_step(),
            default_view: default_view(),
            prefetch: default_true(),
            radio: false,
            mouse: default_true(),
            cava: default_true(),
            cava_position: default_cava_position(),
            cava_bars: default_cava_bars(),
            normalize: false,
            eq: default_eq(),
            eq_gains: Vec::new(),
            audio_device: String::new(),
            mpris: default_true(),
            library_paths: Vec::new(),
            listenbrainz_token: String::new(),
            fade_seconds: 0.0,
            replaygain: "off".to_string(),
            transparent: true,
            wallpaper: String::new(),
            ambient_art: false,
            icons: "nerd".to_string(),
            notifications: false,
        }
    }
}

const DEFAULT_CONFIG: &str = r#"# mfe-player configuration
# Edit and restart the app. Every key is optional.

# Colour theme: mfe | gruvbox | nord | mono | matrix | rose |
#               catppuccin | dracula | tokyo-night | solarized | amber | ocean |
#               auto (your terminal's own 16 colours) | wallpaper
theme = "mfe"

# Playback steps (seconds / percent).
seek_step = 5
long_seek_step = 30
volume_step = 5

# View shown at startup: search | queue | favorites | playlists | downloads | plugins
default_view = "search"

# Resolve the next track while the current one plays (no silence between songs).
prefetch = true

# When the queue ends, keep playing related tracks (Spotify-style radio).
radio = false

# Mouse: wheel scrolls, click selects, double-click plays, click the progress bar to seek.
mouse = true

# Spectrum visualiser (needs `cava` installed).
cava = true
cava_position = "right"   # right | left
cava_bars = 28

# Audio polish.
normalize = false          # loudness normalisation via mpv dynaudnorm
eq = "flat"                # flat | bass | vocal | night | loud
audio_device = ""          # empty = system default (e.g. "alsa/hdmi:CARD=HDMI")

# Desktop media keys / playerctl integration.
mpris = true

# Local music folders (FLAC, MP3, Opus, Ogg, M4A...). Scanned on launch.
# Example: library_paths = ["~/Music", "/mnt/archive/albums"]
library_paths = []

# Optional: scrobble to ListenBrainz (https://listenbrainz.org/profile/).
# Paste your user token here and every play is submitted from a background
# thread. Leave empty to keep everything local.
listenbrainz_token = ""

# Smart fade: seconds of fade at the start and the end of every track.
# 0 = off, 2-4 = gentle, 6 = long. Cycle with Alt+F.
fade_seconds = 0

# ReplayGain for your own files: off | track | album. Cycle with Alt+R.
replaygain = "off"

# Custom ten-band EQ curve in dB. Tune it with Ctrl+E in the app.
eq_gains = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]

# Let the terminal's transparency show your wallpaper through the app.
# On by default; toggle at runtime with Alt+W.
transparent = true

# Optional explicit wallpaper for theme = "wallpaper". Empty means "ask the
# compositor" (swww, hyprpaper, GNOME, Plasma, macOS, Windows).
wallpaper = ""

# Ambient tint: pull a vivid accent from the current album cover and let the
# theme follow the music. Grey covers are ignored. Toggle with Alt+A.
ambient_art = false

# Glyphs: nerd (default), ascii (plain TTYs/screen readers), or auto.
icons = "nerd"

# Desktop notification when a new track starts (Linux, macOS, Windows).
notifications = false
"#;

pub fn load_config() -> (AppConfig, Option<String>) {
    let path = config_path();
    if !path.exists() {
        let _ = fs::write(&path, DEFAULT_CONFIG);
        return (AppConfig::default(), None);
    }
    match fs::read_to_string(&path) {
        Ok(raw) => match toml::from_str::<AppConfig>(&raw) {
            Ok(mut config) => {
                config.seek_step = config.seek_step.clamp(1.0, 600.0);
                config.long_seek_step = config.long_seek_step.clamp(1.0, 600.0);
                config.volume_step = config.volume_step.clamp(1.0, 50.0);
                config.cava_bars = config.cava_bars.clamp(4, 96);
                config.fade_seconds = config.fade_seconds.clamp(0.0, 15.0);
                config.replaygain = config.replaygain.to_ascii_lowercase();
                if !["off", "track", "album"].contains(&config.replaygain.as_str()) {
                    config.replaygain = "off".to_string();
                }
                (config, None)
            }
            Err(err) => (
                AppConfig::default(),
                Some(format!("config.toml is invalid, using defaults: {err}")),
            ),
        },
        Err(err) => (
            AppConfig::default(),
            Some(format!("could not read config.toml: {err}")),
        ),
    }
}

/// Rewrite one `key` line in the config so user comments survive.
fn set_key_in_file(key: &str, value: &str) {
    let path = config_path();
    let Ok(raw) = fs::read_to_string(&path) else {
        return;
    };
    let mut replaced = false;
    let mut out = String::new();
    for line in raw.lines() {
        let starts = line
            .trim_start()
            .strip_prefix(key)
            .and_then(|rest| rest.trim_start().strip_prefix('='));
        if !replaced && starts.is_some() {
            out.push_str(&format!("{key} = {value}\n"));
            replaced = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !replaced {
        out.push_str(&format!("{key} = {value}\n"));
    }
    let _ = fs::write(path, out);
}

/// Rewrite just the `theme` line so user comments in config.toml survive.
pub fn set_theme_in_file(name: &str) {
    set_key_in_file("theme", &format!("\"{name}\""));
}

/// Persist an audio setting that can be cycled from the keyboard.
pub fn set_line(key: &str, value: &str) {
    set_key_in_file(key, value);
}

pub fn ensure_dirs() -> Result<()> {
    fs::create_dir_all(config_dir())
        .with_context(|| format!("creating {}", config_dir().display()))?;
    fs::create_dir_all(plugins_dir())
        .with_context(|| format!("creating {}", plugins_dir().display()))?;
    fs::create_dir_all(runtime_dir())
        .with_context(|| format!("creating {}", runtime_dir().display()))?;
    fs::create_dir_all(downloads_dir())
        .with_context(|| format!("creating {}", downloads_dir().display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transparency_is_the_default() {
        assert!(AppConfig::default().transparent);
        assert!(!AppConfig::default().ambient_art);
    }
}
