//! Headless playback daemon.
//!
//! Ctrl+B used to hand mpv a single file and walk away. That is fine until the
//! song ends: repeat, shuffle, the queue and radio all lived in the TUI process,
//! so "background playback" quietly died after one track. This module keeps a
//! tiny headless copy of the app alive instead. It owns mpv, advances the queue,
//! honours repeat/shuffle/radio, serves MPRIS and persists state. The UI can be
//! killed, SSH can drop, the laptop lid can close — the music keeps going.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde_json::json;

use crate::app::App;
use crate::config;

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    SHUTDOWN.store(true, Ordering::SeqCst);
}

fn pid_alive(pid: u32) -> bool {
    pid != 0 && std::path::Path::new(&format!("/proc/{pid}")).exists()
}

pub fn running_pid() -> Option<u32> {
    fs::read_to_string(config::daemon_pid_path())
        .ok()
        .and_then(|raw| raw.trim().parse::<u32>().ok())
        .filter(|pid| pid_alive(*pid))
}

pub fn is_running() -> bool {
    running_pid().is_some()
}

/// Start the daemon as a fully detached process. Called when the user presses
/// Ctrl+B. The daemon waits for this process to release mpv before attaching.
pub fn spawn_background() -> Result<()> {
    let exe = std::env::current_exe().context("locating the mfe-player binary")?;
    let log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(config::daemon_log_path())
        .ok();

    let mut command = Command::new(exe);
    command
        .arg("--daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    if let Some(log) = log {
        command.stderr(Stdio::from(log));
    } else {
        command.stderr(Stdio::null());
    }
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command.spawn().context("starting the background daemon")?;
    Ok(())
}

/// Stop the daemon and leave mpv itself untouched (the daemon marks its audio
/// engine as detached on shutdown, exactly like Ctrl+B does).
pub fn stop_background() {
    let Some(pid) = running_pid() else {
        let _ = fs::remove_file(config::daemon_pid_path());
        return;
    };
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGTERM);
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline && pid_alive(pid) {
        thread::sleep(Duration::from_millis(40));
    }
    if pid_alive(pid) {
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGKILL);
        }
    }
    let _ = fs::remove_file(config::daemon_pid_path());
}

fn session_owner_alive() -> bool {
    fs::read_to_string(config::session_lock_path())
        .ok()
        .and_then(|raw| raw.trim().parse::<u32>().ok())
        .is_some_and(pid_alive)
}

fn wait_for_session_release(timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !session_owner_alive() {
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    false
}

/// A fresh TUI launch takes the player back from the daemon. The daemon exits
/// without stopping mpv, so the UI can simply attach to the same socket and
/// carry on from the current position.
pub fn takeover() {
    if !is_running() {
        return;
    }
    stop_background();
    let _ = wait_for_session_release(Duration::from_secs(2));
}

/// Entry point for `mfe-player --daemon`.
pub fn run() -> Result<()> {
    config::ensure_dirs()?;
    let _ = fs::write(config::daemon_pid_path(), std::process::id().to_string());
    unsafe {
        libc::signal(libc::SIGTERM, on_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, on_signal as *const () as libc::sighandler_t);
    }

    // The TUI that spawned us still owns the mpv socket for a few milliseconds.
    // Wait for it to go away instead of spawning a second player and ending up
    // with two songs at once.
    if !wait_for_session_release(Duration::from_secs(10)) {
        eprintln!("mfe-player daemon: previous session never released the player");
        let _ = fs::remove_file(config::daemon_pid_path());
        return Ok(());
    }
    // Let the old process finish tearing down its D-Bus/MPRIS name.
    thread::sleep(Duration::from_millis(350));

    let (app_config, warning) = config::load_config();
    let mut app = App::new_headless(app_config)?;
    if let Some(warning) = warning {
        app.flash(warning);
    }
    // Give mpv a beat to report `idle-active` before deciding whether the
    // queue needs a nudge. Without this the daemon can attach, see the
    // snapshot's default `idle = false`, and sit there silently.
    thread::sleep(Duration::from_millis(700));
    app.daemon_resume_if_idle();
    app.persist();

    while !SHUTDOWN.load(Ordering::SeqCst) {
        app.tick();
        thread::sleep(Duration::from_millis(80));
    }

    app.release_audio();
    app.persist();
    let _ = fs::remove_file(config::daemon_pid_path());
    Ok(())
}

/// Send one command to the running mpv over its IPC socket.
pub fn mpv_command(command: serde_json::Value) -> Result<()> {
    let mut stream = UnixStream::connect(config::mpv_socket_path())
        .context("no running player found (start mfe-player first)")?;
    writeln!(stream, "{}", json!({ "command": command }))?;
    stream.flush()?;
    Ok(())
}

/// Ask mpv for a single property and wait for the reply. Used by the CLI
/// integrations (waybar, polybar, tmux) where the state file can be a second
/// behind the actual playback position.
pub fn mpv_property(name: &str) -> Option<serde_json::Value> {
    let mut stream = UnixStream::connect(config::mpv_socket_path()).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .ok()?;
    stream
        .set_write_timeout(Some(Duration::from_millis(500)))
        .ok()?;
    writeln!(
        stream,
        "{}",
        json!({ "command": ["get_property", name], "request_id": 4242 })
    )
    .ok()?;
    stream.flush().ok()?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => return None,
            Ok(_) => {
                let value: serde_json::Value = serde_json::from_str(&line).ok()?;
                if value.get("request_id").and_then(|id| id.as_u64()) == Some(4242) {
                    return value.get("data").cloned();
                }
            }
            Err(_) => return None,
        }
    }
}

/// Machine-readable snapshot for status bars and scripts.
pub fn status_json() -> serde_json::Value {
    let state = crate::state::PersistedState::load().0;
    let title = state
        .now_playing
        .as_ref()
        .map(|track| track.title.clone())
        .unwrap_or_else(|| "nothing playing".to_string());
    let artist = state
        .now_playing
        .as_ref()
        .map(|track| track.artist.clone())
        .unwrap_or_default();
    let album = state
        .now_playing
        .as_ref()
        .map(|track| track.album.clone())
        .unwrap_or_default();

    let connected = mpv_property("idle-active").is_some();
    let paused = mpv_property("pause")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let position = mpv_property("time-pos")
        .and_then(|v| v.as_f64())
        .unwrap_or(state.position);
    let duration = mpv_property("duration")
        .and_then(|v| v.as_f64())
        .unwrap_or_else(|| {
            state
                .now_playing
                .as_ref()
                .and_then(|track| track.duration)
                .map(|d| d as f64)
                .unwrap_or(0.0)
        });
    let mode = if is_running() {
        "daemon"
    } else if connected {
        "session"
    } else {
        "stopped"
    };
    let playing = connected && !paused && duration > 0.0
        || connected && !paused && title != "nothing playing";
    let icon = if !connected {
        "■"
    } else if paused {
        "⏸"
    } else {
        "▶"
    };
    let text = if title == "nothing playing" {
        format!("{icon} idle")
    } else if artist.is_empty() {
        format!("{icon} {title}")
    } else {
        format!("{icon} {title} — {artist}")
    };

    json!({
        "text": text,
        "alt": if !connected { "stopped" } else if paused { "paused" } else { "playing" },
        "tooltip": format!("{title}\n{artist}\n{album}\nmode: {mode} · repeat: {}", state.repeat),
        "class": if !connected { "stopped" } else if paused { "paused" } else { "playing" },
        "title": title,
        "artist": artist,
        "album": album,
        "position": position,
        "duration": duration,
        "paused": paused,
        "playing": playing,
        "repeat": state.repeat,
        "shuffle": state.shuffle,
        "volume": state.volume,
        "mode": mode,
        "connected": connected
    })
}

pub fn stop_everything() -> Result<()> {
    stop_background();
    // `quit` also shuts down a player that was launched in the foreground.
    let _ = mpv_command(json!(["quit"]));
    // mpv can leave the Unix socket behind; a stale socket makes `status`
    // (and the next launch's probe) think a player is still around.
    let _ = fs::remove_file(config::mpv_socket_path());
    let _ = fs::remove_file(config::session_lock_path());
    Ok(())
}

pub fn toggle_pause() -> Result<()> {
    mpv_command(json!(["cycle", "pause"]))
}

pub fn status_line() -> String {
    let state = crate::state::PersistedState::load().0;
    let title = state
        .now_playing
        .as_ref()
        .map(|track| {
            if track.artist.trim().is_empty() {
                track.title.clone()
            } else {
                format!("{} — {}", track.title, track.artist)
            }
        })
        .unwrap_or_else(|| "nothing playing".to_string());
    let mode = if is_running() {
        "background daemon"
    } else if UnixStream::connect(config::mpv_socket_path()).is_ok() {
        "attached session"
    } else {
        "stopped"
    };
    format!("{title}  [{mode}, repeat {}]", state.repeat)
}
