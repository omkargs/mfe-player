use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

use crate::config;
use crate::ipc::{self, Connection};
use crate::models::StreamInfo;

#[derive(Debug, Clone, Default)]
pub struct AudioSnapshot {
    pub position: f64,
    pub duration: f64,
    pub paused: bool,
    pub volume: f64,
    pub idle: bool,
    pub ended: bool,
    pub error: Option<String>,
    pub media_title: Option<String>,
}

pub struct AudioEngine {
    child: Option<Child>,
    writer: Arc<Mutex<Connection>>,
    snapshot: Arc<Mutex<AudioSnapshot>>,
    socket_path: PathBuf,
    attached: bool,
    detached: bool,
    owns_session: bool,
    private: bool,
}

fn pid_alive(pid: u32) -> bool {
    ipc::pid_alive(pid)
}

fn session_owner() -> Option<u32> {
    fs::read_to_string(config::session_lock_path())
        .ok()
        .and_then(|raw| raw.trim().parse::<u32>().ok())
}

impl AudioEngine {
    pub fn connect_or_spawn(volume: f64, audio_device: &str) -> Result<Self> {
        config::ensure_dirs()?;
        let our_pid = std::process::id();

        // Somebody else already owns the shared player: don't steal it out
        // from under them (that used to leave both sessions fighting over one
        // mpv and the music silently "not playing"). Take a private one.
        let private = session_owner().is_some_and(pid_alive);
        let socket_path = if private {
            config::private_mpv_socket_path(our_pid)
        } else {
            config::mpv_socket_path()
        };

        let mut stream = None;
        if !private && socket_path.exists() {
            if let Ok(probe) = probe_socket(&socket_path) {
                stream = Some(probe);
            } else {
                let _ = fs::remove_file(&socket_path);
            }
        }

        let (stream, child, attached) = match stream {
            Some(stream) => (stream, None, true),
            None => {
                let child = spawn_mpv(&socket_path, volume, audio_device)?;
                let stream = wait_for_socket(&socket_path)?;
                (stream, Some(child), false)
            }
        };

        let owns_session = !private;
        if owns_session {
            let _ = fs::write(config::session_lock_path(), our_pid.to_string());
        }

        let writer = Arc::new(Mutex::new(stream.try_clone()?));
        let snapshot = Arc::new(Mutex::new(AudioSnapshot {
            volume,
            ..Default::default()
        }));

        let reader_stream = stream;
        let reader_snapshot = Arc::clone(&snapshot);
        thread::Builder::new()
            .name("mpv-events".to_string())
            .spawn(move || read_events(reader_stream, reader_snapshot))
            .context("starting mpv event reader")?;

        let engine = Self {
            child,
            writer,
            snapshot,
            socket_path,
            attached,
            detached: false,
            owns_session,
            private,
        };

        engine.observe_properties()?;
        engine.set_volume(volume)?;
        Ok(engine)
    }

    pub fn is_attached(&self) -> bool {
        self.attached
    }

    /// True when another live session already owned the shared player, so
    /// this one spawned a private mpv instead of stealing it.
    pub fn is_private(&self) -> bool {
        self.private
    }

    pub fn snapshot(&self) -> AudioSnapshot {
        self.snapshot.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn take_ended(&self) -> bool {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            if snapshot.ended {
                snapshot.ended = false;
                return true;
            }
        }
        false
    }

    pub fn take_error(&self) -> Option<String> {
        self.snapshot.lock().ok().and_then(|mut s| s.error.take())
    }

    pub fn load(&self, info: &StreamInfo) -> Result<()> {
        self.load_from(info, 0.0)
    }

    /// Load a stream and start it at `start` seconds. mpv's `start` is a
    /// per-file option, so it is passed to `loadfile` itself; this lets a
    /// restored session resume mid-song without racing the first position
    /// update.
    pub fn load_from(&self, info: &StreamInfo, start: f64) -> Result<()> {
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.ended = false;
            snapshot.error = None;
            snapshot.position = 0.0;
            snapshot.duration = 0.0;
        }
        // yt-dlp negotiates headers (User-Agent, Referer, Sec-Fetch-*) that the
        // CDN expects. Without them direct googlevideo URLs intermittently 403.
        let fields: Vec<String> = info
            .http_headers
            .iter()
            .filter(|(key, value)| {
                !value.is_empty()
                    && !key.eq_ignore_ascii_case("accept-encoding")
                    && !key.eq_ignore_ascii_case("host")
            })
            .map(|(key, value)| format!("{key}: {}", value.replace([',', '\n', '\r'], " ")))
            .collect();
        let _ = self.command(json!([
            "set_property",
            "http-header-fields",
            fields.join(",")
        ]));

        let start = start.max(0.0);
        if start > 0.5 {
            // The `-1` index keeps the fourth argument in the per-file options
            // slot (mpv >= 0.38).
            self.command(json!([
                "loadfile",
                info.url.as_str(),
                "replace",
                -1,
                { "start": format!("{start:.3}") }
            ]))?;
        } else {
            self.command(json!(["loadfile", info.url.as_str(), "replace"]))?;
        }
        self.set_pause(false)?;
        Ok(())
    }

    pub fn set_pause(&self, paused: bool) -> Result<()> {
        self.command(json!(["set_property", "pause", paused]))?;
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.paused = paused;
        }
        Ok(())
    }

    pub fn toggle_pause(&self) -> Result<()> {
        let paused = self.snapshot().paused;
        self.set_pause(!paused)
    }

    /// Jump to an absolute position (used by clicking the progress bar).
    pub fn seek_absolute(&self, seconds: f64) -> Result<()> {
        let target = seconds.max(0.0);
        self.command(json!(["seek", target, "absolute"]))?;
        Ok(())
    }

    /// Apply an mpv audio filter chain (`af`). An empty string clears it.
    pub fn set_filters(&self, filters: &str) -> Result<()> {
        self.command(json!(["set_property", "af", filters]))?;
        Ok(())
    }

    /// ReplayGain mode (`off`, `track`, `album`).
    pub fn set_replaygain(&self, mode: &str) -> Result<()> {
        self.command(json!(["set_property", "replaygain", mode]))?;
        Ok(())
    }

    pub fn seek_relative(&self, seconds: f64) -> Result<()> {
        let snapshot = self.snapshot();
        let target = (snapshot.position + seconds).max(0.0);
        let duration = snapshot.duration;
        let target = if duration > 0.0 {
            target.min(duration)
        } else {
            target
        };
        self.command(json!(["seek", target, "absolute"]))?;
        Ok(())
    }

    pub fn set_volume(&self, volume: f64) -> Result<()> {
        let volume = volume.clamp(0.0, 130.0);
        self.command(json!(["set_property", "volume", volume]))?;
        if let Ok(mut snapshot) = self.snapshot.lock() {
            snapshot.volume = volume;
        }
        Ok(())
    }

    /// Playback speed (0.5×–2.0×). Great for podcasts and 10-hour mixes.
    pub fn set_speed(&self, speed: f64) -> Result<()> {
        self.command(json!(["set_property", "speed", speed.clamp(0.25, 4.0)]))?;
        Ok(())
    }

    pub fn stop(&self) -> Result<()> {
        self.command(json!(["stop"]))?;
        Ok(())
    }

    pub fn detach(&mut self) {
        self.detached = true;
    }

    /// Hand the player over without stopping it. Used by the headless daemon
    /// when a TUI takes control back: mpv must keep playing across the handoff.
    pub fn release(&mut self) {
        self.detached = true;
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    fn observe_properties(&self) -> Result<()> {
        for (id, name) in [
            (1, "time-pos"),
            (2, "duration"),
            (3, "pause"),
            (4, "volume"),
            (5, "idle-active"),
            (6, "media-title"),
        ] {
            self.command(json!(["observe_property", id, name]))?;
        }
        Ok(())
    }

    fn command(&self, command: Value) -> Result<()> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| anyhow!("mpv IPC writer lock poisoned"))?;
        let line = json!({ "command": command }).to_string();
        writeln!(writer, "{line}").context("writing to mpv IPC")?;
        writer.flush().context("flushing mpv IPC")?;
        Ok(())
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        // Release the shared-player lock (also when detached, so the next
        // launch can re-attach without a second session being locked out).
        if self.owns_session {
            let lock = config::session_lock_path();
            let ours = std::process::id().to_string();
            if fs::read_to_string(&lock)
                .map(|raw| raw.trim() == ours)
                .unwrap_or(false)
            {
                let _ = fs::remove_file(&lock);
            }
        }
        if self.detached {
            return;
        }
        let _ = self.command(json!(["quit"]));
        if let Some(mut child) = self.child.take() {
            let deadline = Instant::now() + Duration::from_millis(400);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if Instant::now() < deadline => {
                        thread::sleep(Duration::from_millis(20));
                    }
                    _ => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break;
                    }
                }
            }
        }
        if !self.attached {
            let _ = fs::remove_file(&self.socket_path);
        }
    }
}

fn spawn_mpv(socket_path: &Path, volume: f64, audio_device: &str) -> Result<Child> {
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let _ = fs::remove_file(socket_path);

    let log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(config::mpv_log_path())
        .ok();

    let mut command = Command::new("mpv");
    command
        .arg("--no-config")
        .arg("--no-video")
        .arg("--force-window=no")
        .arg("--idle=yes")
        .arg("--no-terminal")
        .arg("--msg-level=all=warn")
        .arg("--audio-display=no")
        .arg("--ytdl=yes")
        .arg("--network-timeout=20")
        .arg("--cache=yes")
        .arg("--prefetch-playlist=yes")
        .arg("--gapless-audio=yes")
        .arg("--audio-client-name=mfe-player")
        .arg(format!("--volume={}", volume.clamp(0.0, 130.0)))
        .arg(format!("--input-ipc-server={}", socket_path.display()))
        .stdin(Stdio::null())
        .stdout(Stdio::null());

    if !audio_device.trim().is_empty() {
        command.arg(format!("--audio-device={}", audio_device.trim()));
    }

    if let Some(log) = log {
        command.stderr(Stdio::from(log));
    } else {
        command.stderr(Stdio::null());
    }

    // Detach mpv from the terminal so closing the pane does not kill playback.
    ipc::detach(&mut command);

    command.spawn().map_err(|err| {
        anyhow!(
            "could not launch mpv ({err}). Install mpv, then restart. Search and plugins still work without it."
        )
    })
}

fn wait_for_socket(socket_path: &Path) -> Result<Connection> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(stream) = ipc::connect(socket_path) {
            return Ok(stream);
        }
        if Instant::now() >= deadline {
            return Err(anyhow!(
                "mpv did not create its IPC socket at {}. Check {}",
                socket_path.display(),
                config::mpv_log_path().display()
            ));
        }
        thread::sleep(Duration::from_millis(30));
    }
}

fn probe_socket(path: &Path) -> Result<Connection> {
    let mut stream = ipc::connect(path)?;
    ipc::set_timeouts(&stream, Duration::from_millis(700));
    writeln!(
        stream,
        "{}",
        json!({ "command": ["get_property", "idle-active"], "request_id": 991 })
    )?;
    stream.flush()?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    let deadline = Instant::now() + Duration::from_millis(900);
    while Instant::now() < deadline {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                if let Ok(value) = serde_json::from_str::<Value>(&line) {
                    if value.get("request_id").and_then(Value::as_u64) == Some(991) {
                        ipc::clear_timeouts(&stream);
                        return Ok(stream);
                    }
                }
            }
            Err(err)
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut =>
            {
                break;
            }
            Err(err) => return Err(err.into()),
        }
    }
    Err(anyhow!("stale mpv socket"))
}

fn read_events(stream: Connection, snapshot: Arc<Mutex<AudioSnapshot>>) {
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        match value.get("event").and_then(Value::as_str) {
            Some("property-change") => {
                let Some(name) = value.get("name").and_then(Value::as_str) else {
                    continue;
                };
                let data = value.get("data").cloned().unwrap_or(Value::Null);
                if let Ok(mut snapshot) = snapshot.lock() {
                    match name {
                        "time-pos" => snapshot.position = data.as_f64().unwrap_or(0.0),
                        "duration" => snapshot.duration = data.as_f64().unwrap_or(0.0),
                        "pause" => snapshot.paused = data.as_bool().unwrap_or(false),
                        "volume" => snapshot.volume = data.as_f64().unwrap_or(0.0),
                        "idle-active" => snapshot.idle = data.as_bool().unwrap_or(false),
                        "media-title" => {
                            snapshot.media_title = data.as_str().map(ToString::to_string)
                        }
                        _ => {}
                    }
                }
            }
            Some("end-file") => {
                let reason = value.get("reason").and_then(Value::as_str).unwrap_or("");
                if reason == "eof" || reason == "error" {
                    if let Ok(mut snapshot) = snapshot.lock() {
                        if reason == "error" {
                            snapshot.error = value
                                .get("error")
                                .and_then(Value::as_str)
                                .map(ToString::to_string)
                                .or_else(|| Some("mpv could not play this stream".to_string()));
                        }
                        snapshot.ended = true;
                    }
                }
            }
            Some("file-loaded") => {
                if let Ok(mut snapshot) = snapshot.lock() {
                    snapshot.ended = false;
                    snapshot.error = None;
                }
            }
            _ => {}
        }
    }
    if let Ok(mut snapshot) = snapshot.lock() {
        snapshot.idle = true;
    }
}
