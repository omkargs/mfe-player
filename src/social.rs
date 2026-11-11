//! Little social niceties: share what you are listening to.
//!
//! No accounts, no OAuth dance. `mfe share` (or `Y` in the TUI) copies the
//! current track to the system clipboard and prints it, so you can paste it
//! into chat, a commit message or your own notes.

use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{anyhow, Result};

use crate::models::Track;

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// One-line description used by `mfe share` and the TUI shortcut.
pub fn share_line(track: &Track) -> String {
    let artist = track.artist.trim();
    let album = track.album.trim();
    let mut line = if artist.is_empty() {
        format!("▶ {}", track.title.trim())
    } else {
        format!("▶ {} — {}", track.title.trim(), artist)
    };
    if !album.is_empty() {
        line.push_str(&format!(" ({album})"));
    }
    line.push_str(" · mfe-player");
    line
}

fn pipe_to(program: &str, args: &[&str], text: &str) -> Result<()> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(text.as_bytes())?;
    }
    let status = child.wait()?;
    if !status.success() {
        return Err(anyhow!("{program} exited with {status}"));
    }
    Ok(())
}

/// Copy text to whichever clipboard the session actually has. Wayland first,
/// then X11, then macOS. Returns the tool that worked.
pub fn copy(text: &str) -> Result<&'static str> {
    let candidates: [(&str, &[&str]); 5] = [
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["--clipboard", "--input"]),
        ("pbcopy", &[]),
        ("clip", &[]),
    ];
    let mut last_error = None;
    for (program, args) in candidates {
        match pipe_to(program, args, text) {
            Ok(()) => return Ok(program),
            Err(err) => last_error = Some(err),
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow!("no clipboard tool found")))
}

/// Format and copy in one go. Used by the TUI.
pub fn share(track: &Track) -> Result<String> {
    let line = share_line(track);
    copy(&line)?;
    Ok(line)
}

const LISTENBRAINZ_ENDPOINT: &str = "https://api.listenbrainz.org/1/submit-listens";

fn track_metadata(track: &Track) -> serde_json::Value {
    let mut info = serde_json::json!({
        "submission_client": "mfe-player",
    });
    if let Some(duration) = track.duration {
        info["duration_ms"] = serde_json::json!(duration.saturating_mul(1000));
    }
    if !track.album.trim().is_empty() {
        info["release_name"] = serde_json::json!(track.album.trim());
    }
    serde_json::json!({
        "artist_name": track.artist.trim(),
        "track_name": track.title.trim(),
        "additional_info": info,
    })
}

fn listen_body(track: &Track, listen_type: &str, listened_at: Option<u64>) -> serde_json::Value {
    let mut entry = serde_json::json!({ "track_metadata": track_metadata(track) });
    if let Some(at) = listened_at {
        entry["listened_at"] = serde_json::json!(at);
    }
    serde_json::json!({
        "listen_type": listen_type,
        "payload": [entry],
    })
}

fn submit_async(token: String, body: serde_json::Value) {
    std::thread::Builder::new()
        .name("listenbrainz".to_string())
        .spawn(move || {
            use std::process::{Command, Stdio};
            let Ok(mut child) = Command::new("curl")
                .args([
                    "-fsS",
                    "--max-time",
                    "10",
                    "-X",
                    "POST",
                    "-H",
                    &format!("Authorization: Token {token}"),
                    "-H",
                    "Content-Type: application/json",
                    "--data-binary",
                    "@-",
                    LISTENBRAINZ_ENDPOINT,
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            else {
                return;
            };
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(body.to_string().as_bytes());
            }
            let _ = child.wait();
        })
        .ok();
}

/// Minimal ListenBrainz client. A token in the config is all it needs; the
/// network calls happen on a detached thread so playback never waits on it.
#[derive(Debug, Default)]
pub struct Scrobbler {
    token: String,
    current_id: Option<String>,
    started_at: Option<u64>,
    listen_sent: bool,
}

impl Scrobbler {
    pub fn new(token: String) -> Self {
        Self {
            token,
            ..Self::default()
        }
    }

    pub fn enabled(&self) -> bool {
        !self.token.trim().is_empty()
    }

    /// Call whenever a track starts (including resuming an old one).
    pub fn on_start(&mut self, track: &Track) {
        if !self.enabled() {
            return;
        }
        let now = now_unix();
        if self.current_id.as_deref() == Some(track.id.as_str()) {
            return;
        }
        self.current_id = Some(track.id.clone());
        self.started_at = Some(now);
        self.listen_sent = false;
        submit_async(self.token.clone(), listen_body(track, "playing_now", None));
    }

    /// Called from the app tick. Submits the real listen once the track has
    /// been played long enough for ListenBrainz to count it.
    pub fn tick(&mut self, track: &Track, position: f64) {
        if !self.enabled() || self.listen_sent {
            return;
        }
        if self.current_id.as_deref() != Some(track.id.as_str()) {
            self.on_start(track);
            return;
        }
        let duration = track.duration.unwrap_or(0) as f64;
        let threshold = if duration > 0.0 {
            (duration * 0.5).clamp(30.0, 240.0)
        } else {
            30.0
        };
        if position < threshold {
            return;
        }
        self.listen_sent = true;
        let at = self.started_at.unwrap_or_else(now_unix);
        submit_async(self.token.clone(), listen_body(track, "single", Some(at)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> Track {
        Track {
            id: "test".to_string(),
            title: "Get Lucky".to_string(),
            artist: "Daft Punk".to_string(),
            album: "Random Access Memories".to_string(),
            duration: Some(248),
            artwork: None,
            source: "test".to_string(),
            year: None,
            genre: Vec::new(),
            tags: Vec::new(),
            auto_queued: false,
        }
    }

    #[test]
    fn formats_a_share_line() {
        assert_eq!(
            share_line(&track()),
            "▶ Get Lucky — Daft Punk (Random Access Memories) · mfe-player"
        );
    }

    #[test]
    fn builds_a_listenbrainz_payload() {
        let body = listen_body(&track(), "single", Some(1_700_000_000));
        assert_eq!(body["listen_type"], "single");
        let entry = &body["payload"][0];
        assert_eq!(entry["listened_at"], 1_700_000_000u64);
        assert_eq!(entry["track_metadata"]["artist_name"], "Daft Punk");
        assert_eq!(entry["track_metadata"]["track_name"], "Get Lucky");
        assert_eq!(
            entry["track_metadata"]["additional_info"]["duration_ms"],
            248_000
        );

        let now = listen_body(&track(), "playing_now", None);
        assert!(now["payload"][0].get("listened_at").is_none());
    }

    #[test]
    fn survives_missing_artist_and_album() {
        let mut track = track();
        track.artist = String::new();
        track.album = String::new();
        assert_eq!(share_line(&track), "▶ Get Lucky · mfe-player");
    }
}
