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
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde_json::json;

use crate::app::App;
use crate::config;
use crate::ipc;

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn on_signal(_: libc::c_int) {
    SHUTDOWN.store(true, Ordering::SeqCst);
}

fn pid_alive(pid: u32) -> bool {
    ipc::pid_alive(pid)
}

/// Seconds a heartbeat may be stale before the daemon is considered gone.
const HEARTBEAT_TTL: u64 = 15;

fn heartbeat_is_fresh(pid: u32) -> bool {
    let Ok(raw) = fs::read_to_string(config::daemon_heartbeat_path()) else {
        return false;
    };
    let mut parts = raw.split_whitespace();
    let recorded: Option<u32> = parts.next().and_then(|value| value.parse().ok());
    let stamp: Option<u64> = parts.next().and_then(|value| value.parse().ok());
    if recorded != Some(pid) {
        return false;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    stamp.is_some_and(|stamp| now.saturating_sub(stamp) <= HEARTBEAT_TTL)
}

fn write_heartbeat(pid: u32) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let _ = fs::write(config::daemon_heartbeat_path(), format!("{pid} {now}"));
}

/// The pid file alone can lie (crashes leave it behind, and Windows reuses
/// pids aggressively). A live process *and* a fresh heartbeat are required.
pub fn running_pid() -> Option<u32> {
    let pid = fs::read_to_string(config::daemon_pid_path())
        .ok()
        .and_then(|raw| raw.trim().parse::<u32>().ok())?;
    if !pid_alive(pid) {
        return None;
    }
    if heartbeat_is_fresh(pid) {
        return Some(pid);
    }
    // Upgrade window: an older daemon wrote a pid file but no heartbeat yet.
    // Trust a recently-created pid file for a few seconds so it can be asked
    // to stop gracefully instead of being orphaned.
    let recent = fs::metadata(config::daemon_pid_path())
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.elapsed().ok())
        .is_some_and(|age| age < Duration::from_secs(30));
    recent.then_some(pid)
}

/// Ask the daemon to stop by dropping a marker file. The daemon drains the
/// queue, persists state and exits on its own; no `taskkill`, no lost writes.
fn request_stop() {
    let _ = fs::write(config::daemon_stop_path(), std::process::id().to_string());
}

pub fn is_running() -> bool {
    running_pid().is_some()
}

/// Start the daemon as a fully detached process. Called when the user presses
/// Ctrl+B. The daemon waits for this process to release mpv before attaching.
pub fn spawn_background() -> Result<()> {
    // Ctrl+B followed by a closed window (or a failed draw) can ask twice.
    // One daemon is enough — a second one would just wait ten seconds and exit.
    if is_running() {
        return Ok(());
    }
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
    ipc::detach(&mut command);
    command.spawn().context("starting the background daemon")?;
    Ok(())
}

/// Stop the daemon and leave mpv itself untouched (the daemon marks its audio
/// engine as detached on shutdown, exactly like Ctrl+B does).
pub fn stop_background() {
    let Some(pid) = running_pid() else {
        let _ = fs::remove_file(config::daemon_pid_path());
        let _ = fs::remove_file(config::daemon_heartbeat_path());
        let _ = fs::remove_file(config::daemon_stop_path());
        return;
    };
    // Graceful path first: the daemon notices the marker on its next tick and
    // saves state before exiting. Only fall back to a signal when it is wedged.
    request_stop();
    let graceful = Instant::now() + Duration::from_secs(4);
    while Instant::now() < graceful && pid_alive(pid) {
        thread::sleep(Duration::from_millis(80));
    }
    if !pid_alive(pid) {
        let _ = fs::remove_file(config::daemon_pid_path());
        let _ = fs::remove_file(config::daemon_heartbeat_path());
        let _ = fs::remove_file(config::daemon_stop_path());
        return;
    }
    let _ = fs::remove_file(config::daemon_stop_path());
    ipc::terminate(pid);
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline && pid_alive(pid) {
        thread::sleep(Duration::from_millis(40));
    }
    if pid_alive(pid) {
        ipc::kill(pid);
    }
    let _ = fs::remove_file(config::daemon_pid_path());
}

fn session_owner_alive() -> bool {
    fs::read_to_string(config::session_lock_path())
        .ok()
        .and_then(|raw| raw.trim().parse::<u32>().ok())
        .is_some_and(ipc::is_player_pid)
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

/// Remove state left by sessions that died without a clean shutdown: a
/// `session.pid` whose pid now belongs to some unrelated program, and private
/// `mpv-<pid>.sock` files from players that are gone. Without this, pid reuse
/// makes every new launch think another session owns the player.
pub fn cleanup_stale_state() {
    let lock = config::session_lock_path();
    if let Some(pid) = fs::read_to_string(&lock)
        .ok()
        .and_then(|raw| raw.trim().parse::<u32>().ok())
    {
        if !ipc::is_player_pid(pid) {
            let _ = fs::remove_file(&lock);
        }
    }
    let Ok(entries) = fs::read_dir(config::runtime_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(pid) = name
            .strip_prefix("mpv-")
            .and_then(|rest| rest.strip_suffix(".sock"))
            .and_then(|pid| pid.parse::<u32>().ok())
        else {
            continue;
        };
        if !ipc::is_player_pid(pid) {
            let _ = fs::remove_file(entry.path());
        }
    }
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
    let _ = fs::remove_file(config::daemon_stop_path());
    // A command left over from a previous run must not be replayed now.
    let _ = fs::remove_file(config::daemon_control_path());
    let _ = fs::write(config::daemon_pid_path(), std::process::id().to_string());
    write_heartbeat(std::process::id());
    #[cfg(unix)]
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

    let mut last_beat = Instant::now();
    while !SHUTDOWN.load(Ordering::SeqCst) {
        if config::daemon_stop_path().exists() {
            break;
        }
        app.tick();
        let commands = drain_control();
        if !commands.is_empty() {
            app.apply_remote(&commands);
        }
        if last_beat.elapsed() >= Duration::from_secs(1) {
            write_heartbeat(std::process::id());
            last_beat = Instant::now();
        }
        thread::sleep(Duration::from_millis(80));
    }

    app.release_audio();
    app.persist();
    let _ = fs::remove_file(config::daemon_pid_path());
    let _ = fs::remove_file(config::daemon_heartbeat_path());
    let _ = fs::remove_file(config::daemon_stop_path());
    let _ = fs::remove_file(config::daemon_control_path());
    Ok(())
}

/// Send one command to the running mpv over its IPC socket.
pub fn mpv_command(command: serde_json::Value) -> Result<()> {
    let mut stream = ipc::connect(&config::mpv_socket_path())
        .context("no running player found (start mfe-player first)")?;
    writeln!(stream, "{}", json!({ "command": command }))?;
    stream.flush()?;
    Ok(())
}

/// Ask mpv for a single property and wait for the reply. Used by the CLI
/// integrations (waybar, polybar, tmux) where the state file can be a second
/// behind the actual playback position.
pub fn mpv_property(name: &str) -> Option<serde_json::Value> {
    let mut stream = ipc::connect(&config::mpv_socket_path()).ok()?;
    ipc::set_timeouts(&stream, Duration::from_millis(500));
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

/// Ask mpv for several properties over one socket connection. Status bars
/// poll this every second; four separate connects per poll is pure overhead.
pub fn mpv_properties(
    names: &[&str],
) -> std::collections::HashMap<String, serde_json::Value> {
    use std::collections::HashMap;

    let mut out: HashMap<String, serde_json::Value> = HashMap::new();
    let Ok(mut stream) = ipc::connect(&config::mpv_socket_path()) else {
        return out;
    };
    ipc::set_timeouts(&stream, Duration::from_millis(500));
    for (index, name) in names.iter().enumerate() {
        let request_id = 5000u64 + index as u64;
        if writeln!(
            stream,
            "{}",
            json!({ "command": ["get_property", name], "request_id": request_id })
        )
        .is_err()
        {
            return out;
        }
    }
    if stream.flush().is_err() {
        return out;
    }
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    while out.len() < names.len() {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                    continue;
                };
                let Some(id) = value.get("request_id").and_then(|id| id.as_u64()) else {
                    continue;
                };
                let Some(index) = id.checked_sub(5000).map(|id| id as usize) else {
                    continue;
                };
                if index >= names.len() {
                    continue;
                }
                let Some(data) = value.get("data") else {
                    continue;
                };
                if data.is_null() {
                    continue;
                }
                out.insert(names[index].to_string(), data.clone());
            }
            Err(_) => break,
        }
    }
    out
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

    let props = mpv_properties(&["idle-active", "pause", "time-pos", "duration"]);
    let connected = props.contains_key("idle-active");
    let paused = props
        .get("pause")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let position = props
        .get("time-pos")
        .and_then(|v| v.as_f64())
        .unwrap_or(state.position);
    let duration = props
        .get("duration")
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
    let _ = fs::remove_file(config::daemon_control_path());
    Ok(())
}

pub fn toggle_pause() -> Result<()> {
    control("toggle")
}

/// True when either the headless daemon or an open TUI owns the player. Both
/// drain the same command file, so `mfe next` works whichever one is up.
pub fn player_alive() -> bool {
    is_running() || session_owner_alive()
}

/// Append a command for whichever player process is running. Returns false
/// when nobody is listening, so callers can fall back to talking to mpv.
pub fn send_control(command: &str) -> bool {
    if !player_alive() {
        return false;
    }
    let path = config::daemon_control_path();
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| writeln!(file, "{command}"))
        .is_ok()
}

/// Drain the command inbox on the player's tick. The file is emptied first so
/// a command is applied once even if two `mfe` calls race each other.
pub fn drain_control() -> Vec<String> {
    let path = config::daemon_control_path();
    let Ok(raw) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    if raw.trim().is_empty() {
        return Vec::new();
    }
    let _ = fs::write(&path, "");
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// Commands a plain mpv session can apply without the queue. Everything else
/// (next/previous/shuffle/repeat) needs the daemon's bookkeeping.
fn direct_command(command: &str) -> Option<serde_json::Value> {
    let mut parts = command.split_whitespace();
    let name = parts.next()?;
    let arg = parts.next();
    let number = || arg.and_then(|raw| raw.parse::<f64>().ok());
    Some(match name {
        "play" => json!(["set_property", "pause", false]),
        "pause" => json!(["set_property", "pause", true]),
        "toggle" => json!(["cycle", "pause"]),
        "mute" => json!(["cycle", "mute"]),
        "seek" => json!(["seek", number()?, "relative"]),
        "volume" => json!(["set_property", "volume", number()?]),
        _ => return None,
    })
}

/// Send a command to the daemon, or directly to a foreground mpv session when
/// the command does not depend on the queue.
pub fn control(command: &str) -> Result<()> {
    if send_control(command) {
        return Ok(());
    }
    match direct_command(command) {
        Some(command) => mpv_command(command),
        None => anyhow::bail!(
            "no player running — open mfe, or press Ctrl+B in it to hand playback to the daemon"
        ),
    }
}

/// `mfe volume [N | +N | -N | up | down | mute]`. Reads when no argument is
/// given, so status bars can call it without extra plumbing. Values are
/// clamped to the configured `max_volume`.
pub fn volume_command(arg: Option<&str>) -> Result<()> {
    let Some(volume) = mpv_property("volume").and_then(|value| value.as_f64()) else {
        anyhow::bail!("no running player found (start mfe-player first)");
    };
    let current = volume;
    let (config, _) = crate::config::load_config();
    let max = config.max_volume;
    let step = config.volume_step;
    let Some(arg) = arg else {
        println!("{current:.0}");
        return Ok(());
    };
    let target = match arg.trim() {
        "" => current,
        "mute" => 0.0,
        "up" => current + step,
        "down" => current - step,
        raw => match raw.parse::<f64>() {
            Ok(value) if raw.starts_with('+') || raw.starts_with('-') => current + value,
            Ok(value) => value,
            Err(_) => anyhow::bail!("volume expects a number, +N, -N, up, down or mute"),
        },
    };
    let target = target.clamp(0.0, max);
    // Route through the daemon when it owns the player so its state (and the
    // next fade) uses the new value instead of reverting it.
    if !send_control(&format!("volume {target:.0}")) {
        mpv_command(json!(["set_property", "volume", target]))?;
    }
    println!("{target:.0}");
    Ok(())
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
    } else if ipc::connect(&config::mpv_socket_path()).is_ok() {
        "attached session"
    } else {
        "stopped"
    };
    format!("{title}  [{mode}, repeat {}]", state.repeat)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_commands_cover_state_without_a_queue() {
        assert!(direct_command("toggle").is_some());
        assert!(direct_command("pause").is_some());
        assert!(direct_command("play").is_some());
        assert!(direct_command("mute").is_some());
        assert!(direct_command("seek +10").is_some());
        assert!(direct_command("seek -30.5").is_some());
        assert!(direct_command("volume 65").is_some());
    }

    #[test]
    fn queue_commands_require_the_player_process() {
        for command in ["next", "prev", "shuffle on", "repeat one", "seek +x"] {
            assert!(direct_command(command).is_none(), "{command}");
        }
    }
}
