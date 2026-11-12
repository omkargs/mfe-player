//! `mfe doctor`: print what is installed, what is missing, and the exact fix.
//!
//! Bug reports without this always start with three questions. Now the answer
//! is one command.

use std::env;
use std::path::PathBuf;

use crate::{config, daemon, ipc, plugins, wallpaper};

pub fn run() -> i32 {
    println!("mfe-player doctor");
    println!();

    let mut broken = 0;

    println!("binaries");
    if !require_binary(
        "mpv",
        "audio playback",
        "install mpv with your package manager",
    ) {
        broken += 1;
    }
    match find_binary("python3").or_else(|| find_binary("python")) {
        Some(python) => println!(
            "  ok {}   plugin runtime",
            python
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("python")
        ),
        None => {
            println!("  x python3    plugins cannot run — install python3");
            broken += 1;
        }
    }
    optional_binary(
        "yt-dlp",
        "streaming",
        "cargo install yt-dlp (or pipx install yt-dlp)",
    );
    optional_binary("ffmpeg", "downloads", "install ffmpeg");
    #[cfg(target_os = "linux")]
    {
        optional_binary("cava", "spectrum visualiser", "install cava");
        optional_binary("playerctl", "media keys", "install playerctl");
    }

    println!();
    println!("configuration");
    let config_dir = config::config_dir();
    println!("  ·  {}", config_dir.display());
    if config_dir.exists() {
        println!("  ok config directory exists");
    } else {
        println!("  !  config directory is created on first launch");
    }
    let (app_config, warning) = config::load_config();
    match warning {
        Some(warning) => {
            println!("  !  {warning}");
            broken += 1;
        }
        None => println!("  ok config parses (theme: {})", app_config.theme),
    }

    let registry = plugins::PluginRegistry::load();
    let bundled = plugins::catalog()
        .into_iter()
        .filter(|plugin| {
            !registry
                .installed
                .iter()
                .any(|installed| installed.manifest.id == plugin.manifest.id)
        })
        .count();
    if registry.installed.is_empty() {
        println!("  !  no plugins installed yet — they install on first launch");
    } else {
        println!(
            "  ok {} plugin(s) installed{}",
            registry.installed.len(),
            if bundled > 0 {
                format!(", {bundled} bundled plugin(s) will install on next launch")
            } else {
                String::new()
            }
        );
    }

    println!();
    println!("playback");
    if daemon::is_running() {
        println!("  ok background daemon is running");
    } else if ipc::connect(&config::mpv_socket_path()).is_ok() {
        println!("  ok mpv is attached (TUI owns it)");
    } else {
        println!("  ·  mpv is not running right now (fine)");
    }
    #[cfg(target_os = "linux")]
    if app_config.mpris {
        println!("  ok MPRIS enabled (Linux media keys)");
    }

    println!();
    println!("desktop");
    let clipboard = ["wl-copy", "xclip", "xsel", "pbcopy", "clip"]
        .iter()
        .find(|tool| find_binary(tool).is_some());
    match clipboard {
        Some(tool) => println!("  ok clipboard via {tool}"),
        None => println!("  !  no clipboard tool found — mfe share will fail"),
    }
    if app_config.discord {
        let id = if !app_config.discord_client_id.trim().is_empty() {
            app_config.discord_client_id.trim().to_string()
        } else {
            std::env::var("MFE_DISCORD_CLIENT_ID").unwrap_or_default()
        };
        if id.is_empty() {
            println!(
                "  !  discord presence is on but has no client id — run `mfe discord client <id>`"
            );
        } else if crate::discord::socket_present() {
            println!("  ok discord presence (desktop client detected)");
        } else {
            println!("  ·  discord presence configured — desktop client not running");
        }
    }
    match wallpaper::detect() {
        Some(path) => println!("  ok wallpaper: {}", path.display()),
        None => println!(
            "  ·  no wallpaper detected — set wallpaper = \"/path\" for theme = \"wallpaper\""
        ),
    }
    let truecolor = env::var("COLORTERM")
        .map(|value| {
            let value = value.to_ascii_lowercase();
            value.contains("truecolor") || value.contains("24bit")
        })
        .unwrap_or(false);
    println!(
        "  {} terminal colour: {}",
        if truecolor { "ok" } else { "·" },
        if truecolor {
            "24-bit"
        } else {
            "unknown (themes will approximate)"
        }
    );

    println!();
    if broken == 0 {
        println!("all required pieces are in place.");
        0
    } else {
        println!("{broken} required piece(s) missing — fix the x lines above.");
        1
    }
}

fn require_binary(name: &str, purpose: &str, hint: &str) -> bool {
    match find_binary(name) {
        Some(path) => {
            println!("  ok {name:<10} {purpose} ({})", path.display());
            true
        }
        None => {
            println!("  x  {name:<10} missing — {purpose}. {hint}");
            false
        }
    }
}

fn optional_binary(name: &str, purpose: &str, hint: &str) {
    match find_binary(name) {
        Some(_) => println!("  ok {name:<10} {purpose}"),
        None => println!("  ·  {name:<10} not found — {purpose}. {hint}"),
    }
}

pub fn find_binary(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .flat_map(|dir| {
            #[allow(unused_mut)]
            let mut candidates = vec![dir.join(name)];
            #[cfg(windows)]
            {
                candidates.push(dir.join(format!("{name}.exe")));
                candidates.push(dir.join(format!("{name}.cmd")));
                candidates.push(dir.join(format!("{name}.bat")));
            }
            candidates
        })
        .find(|candidate| candidate.is_file())
}
