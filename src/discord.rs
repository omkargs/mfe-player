//! Local Discord Rich Presence.
//!
//! Discord exposes a tiny local IPC socket to desktop apps: an 8-byte header
//! (opcode + length, both little-endian) followed by a JSON body. The first
//! frame is a handshake carrying the application's client id; after that every
//! `SET_ACTIVITY` frame updates "Listening to …".
//!
//! Everything here is local: no bot token, no webhook, no server. Presence only
//! appears when the Discord desktop client is running and the user configured
//! `discord = true` plus a `discord_client_id` in config.toml.

use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::ipc;
use crate::models::Track;

const OP_HANDSHAKE: u32 = 0;
const OP_FRAME: u32 = 1;
/// Discord truncates these fields at 128 characters; do it up front so the
/// signature we use for de-duplication matches what is actually displayed.
const MAX_FIELD: usize = 128;
const SOCKET_COUNT: usize = 10;
/// Discord's activity type for "Listening to …".
const ACTIVITY_LISTENING: u8 = 2;
const REPO_URL: &str = "https://github.com/omkargs/mfe-player";

/// Handle used by the UI thread. Dropping it closes the channel, which lets the
/// worker exit and disconnect (clearing the presence).
pub struct Discord {
    updates: SyncSender<Option<Activity>>,
    _thread: thread::JoinHandle<()>,
}

impl Discord {
    pub fn start(client_id: String) -> Self {
        // Small bounded queue: presence changes are rare (deduplicated by
        // signature), and a full queue means Discord is not keeping up, so
        // dropping the newest update is better than blocking the UI thread.
        let (updates, receiver) = mpsc::sync_channel(4);
        let thread = thread::Builder::new()
            .name("discord-presence".to_string())
            .spawn(move || run(client_id, receiver))
            .expect("starting Discord presence thread");
        Self {
            updates,
            _thread: thread,
        }
    }

    /// Queue a presence update, or `None` to clear it. Never blocks the UI: if
    /// the worker is busy or Discord is down the update is simply dropped.
    pub fn push(&self, activity: Option<Activity>) {
        match self.updates.try_send(activity) {
            Ok(()) | Err(mpsc::TrySendError::Full(_)) => {}
            Err(mpsc::TrySendError::Disconnected(_)) => {}
        }
    }
}

/// What friends see on the Discord profile.
#[derive(Debug, Clone, PartialEq)]
pub struct Activity {
    pub details: String,
    pub state: String,
    pub started_at: u64,
    pub ends_at: Option<u64>,
    pub large_image: String,
    pub large_text: String,
    pub small_image: String,
    pub small_text: String,
    pub paused: bool,
    pub buttons: bool,
}

impl Activity {
    pub fn from_track(track: &Track, position: f64, paused: bool, asset: &str) -> Self {
        let now = now_unix();
        let position = position.clamp(0.0, 86_400.0) as u64;
        let started_at = now.saturating_sub(position);
        let duration = track.duration.unwrap_or(0);
        let ends_at = (duration > 0).then_some(started_at.saturating_add(duration));

        let mut state = if track.album.trim().is_empty() {
            track.artist.clone()
        } else {
            format!("{} · {}", track.artist, track.album)
        };
        if paused {
            if state.is_empty() {
                state = "paused".to_string();
            } else {
                state.push_str(" · paused");
            }
        }

        let large_text = if track.album.trim().is_empty() {
            track.title.clone()
        } else {
            track.album.clone()
        };

        Self {
            details: trim_field(&track.title),
            state: trim_field(&state),
            started_at,
            ends_at,
            large_image: asset.trim().to_string(),
            large_text: trim_field(&large_text),
            small_image: if paused { "pause" } else { "play" }.to_string(),
            small_text: if paused { "Paused" } else { "Playing" }.to_string(),
            paused,
            buttons: true,
        }
    }

    /// Changes only when the visible presence should change. Position is
    /// bucketed so seeks eventually re-sync the progress bar without pushing a
    /// frame every second.
    pub fn signature(&self, position: f64) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}",
            self.details,
            self.state,
            self.large_text,
            self.large_image,
            self.small_image,
            self.paused,
            self.buttons,
            position.clamp(0.0, 86_400.0) as u64 / 10
        )
    }

    fn payload(&self, pid: u32) -> Value {
        let mut activity = json!({
            "type": ACTIVITY_LISTENING,
            "details": self.details,
            "state": self.state,
            "assets": {
                "large_image": self.large_image,
                "large_text": self.large_text,
                "small_image": self.small_image,
                "small_text": self.small_text,
            },
            "instance": false,
        });
        if self.buttons {
            // Discord allows at most two buttons; both link to the project so
            // friends can find the player without any account or token.
            activity["buttons"] = json!([
                { "label": "Get mfe-player", "url": REPO_URL },
                { "label": "How it works", "url": format!("{REPO_URL}#readme") },
            ]);
        }
        activity["timestamps"] = match self.ends_at {
            Some(end) => json!({ "start": self.started_at, "end": end }),
            None => json!({ "start": self.started_at }),
        };
        json!({
            "cmd": "SET_ACTIVITY",
            "args": { "pid": pid, "activity": activity },
            "nonce": format!("{}-{pid}", self.started_at),
        })
    }
}

fn run(client_id: String, receiver: Receiver<Option<Activity>>) {
    let mut connection: Option<ipc::Connection> = None;
    loop {
        match receiver.recv_timeout(Duration::from_secs(15)) {
            Ok(update) => {
                if connection.is_none() {
                    connection = connect(&client_id);
                }
                let Some(stream) = connection.as_mut() else {
                    continue;
                };
                let frame = match update {
                    Some(activity) => activity.payload(std::process::id()),
                    None => json!({
                        "cmd": "SET_ACTIVITY",
                        "args": { "pid": std::process::id() },
                        "nonce": "clear",
                    }),
                };
                if write_frame(stream, OP_FRAME, &frame).is_err() {
                    // Discord probably restarted; reconnect on the next update.
                    connection = None;
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if connection.is_none() {
                    connection = connect(&client_id);
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn connect(client_id: &str) -> Option<ipc::Connection> {
    for path in socket_paths() {
        let Ok(mut stream) = ipc::connect(&path) else {
            continue;
        };
        ipc::set_timeouts(&stream, Duration::from_secs(2));
        let handshake = json!({ "v": 1, "client_id": client_id });
        if write_frame(&mut stream, OP_HANDSHAKE, &handshake).is_err() {
            continue;
        }
        if read_frame(&mut stream).is_err() {
            continue;
        }
        ipc::clear_timeouts(&stream);
        return Some(stream);
    }
    None
}

/// True when any Discord IPC endpoint exists. Cheap check used by `mfe doctor`
/// and `mfe discord status`; it never performs a handshake.
pub fn socket_present() -> bool {
    socket_paths().iter().any(|path| {
        #[cfg(unix)]
        {
            path.exists()
        }
        #[cfg(windows)]
        {
            std::fs::File::open(path).is_ok()
        }
    })
}

/// Every place the Discord desktop client may have created its socket,
/// including Flatpak/Snap sandboxes.
fn socket_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();

    #[cfg(windows)]
    {
        for index in 0..SOCKET_COUNT {
            out.push(PathBuf::from(format!(r"\\.\pipe\discord-ipc-{index}")));
        }
    }

    #[cfg(unix)]
    {
        let mut dirs = Vec::new();
        for key in ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"] {
            if let Ok(value) = std::env::var(key) {
                if !value.trim().is_empty() {
                    dirs.push(PathBuf::from(value));
                }
            }
        }
        dirs.push(PathBuf::from("/tmp"));
        dirs.dedup();
        for dir in dirs {
            for index in 0..SOCKET_COUNT {
                out.push(dir.join(format!("discord-ipc-{index}")));
                out.push(dir.join(format!("app/com.discordapp.Discord/discord-ipc-{index}")));
                out.push(dir.join(format!("snap.discord/discord-ipc-{index}")));
            }
        }
    }

    out.dedup();
    out
}

fn write_frame<W: Write>(writer: &mut W, op: u32, payload: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(payload).map_err(io::Error::other)?;
    let length = u32::try_from(body.len()).map_err(|_| io::Error::other("frame too large"))?;
    writer.write_all(&op.to_le_bytes())?;
    writer.write_all(&length.to_le_bytes())?;
    writer.write_all(&body)?;
    writer.flush()
}

fn read_frame<R: Read>(reader: &mut R) -> io::Result<(u32, Value)> {
    let mut header = [0u8; 8];
    reader.read_exact(&mut header)?;
    let op = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    let length = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    if length > 1 << 20 {
        return Err(io::Error::other("Discord frame too large"));
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    let value = serde_json::from_slice(&body).map_err(io::Error::other)?;
    Ok((op, value))
}

fn trim_field(raw: &str) -> String {
    raw.trim().chars().take(MAX_FIELD).collect()
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> Track {
        Track {
            id: "test".to_string(),
            title: "Midnight Phonk".to_string(),
            artist: "DJ Test".to_string(),
            album: "Night Drive".to_string(),
            duration: Some(200),
            artwork: None,
            source: "youtube".to_string(),
            year: None,
            genre: Vec::new(),
            tags: Vec::new(),
            auto_queued: false,
        }
    }

    #[test]
    fn timestamps_follow_position_and_duration() {
        let before = now_unix();
        let activity = Activity::from_track(&track(), 50.0, false, "mfe");
        let after = now_unix();
        assert!(activity.started_at >= before.saturating_sub(50));
        assert!(activity.started_at <= after.saturating_sub(50));
        assert_eq!(activity.ends_at, Some(activity.started_at + 200));
    }

    #[test]
    fn paused_tracks_say_so() {
        let activity = Activity::from_track(&track(), 10.0, true, "mfe");
        assert!(activity.state.ends_with("paused"), "{}", activity.state);
    }

    #[test]
    fn signature_ignores_small_seek_changes_but_tracks_track_changes() {
        let first = Activity::from_track(&track(), 10.0, false, "mfe");
        let second = Activity::from_track(&track(), 12.0, false, "mfe");
        assert_eq!(first.signature(10.0), second.signature(12.0));

        let mut other = track();
        other.title = "Something Else".to_string();
        let third = Activity::from_track(&other, 10.0, false, "mfe");
        assert_ne!(first.signature(10.0), third.signature(10.0));
    }

    #[test]
    fn long_fields_are_trimmed_to_discord_limits() {
        let mut long = track();
        long.title = "x".repeat(500);
        let activity = Activity::from_track(&long, 0.0, false, "mfe");
        assert_eq!(activity.details.chars().count(), MAX_FIELD);
    }

    #[test]
    fn frames_round_trip() {
        let payload = json!({ "cmd": "SET_ACTIVITY", "args": { "pid": 7 } });
        let mut buffer = Vec::new();
        write_frame(&mut buffer, OP_FRAME, &payload).unwrap();
        let (op, body) = read_frame(&mut buffer.as_slice()).unwrap();
        assert_eq!(op, OP_FRAME);
        assert_eq!(body, payload);
    }

    #[test]
    fn oversized_frames_are_rejected() {
        let mut header = Vec::new();
        header.extend_from_slice(&OP_FRAME.to_le_bytes());
        header.extend_from_slice(&(2u32 << 20).to_le_bytes());
        assert!(read_frame(&mut header.as_slice()).is_err());
    }
}
