//! Little social niceties: share what you are listening to.
//!
//! No accounts, no OAuth dance. `mfe share` (or `Y` in the TUI) copies the
//! current track to the system clipboard and prints it, so you can paste it
//! into chat, a commit message or your own notes.

use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{anyhow, Result};

use crate::models::Track;

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
    let candidates: [(&str, &[&str]); 4] = [
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["--clipboard", "--input"]),
        ("pbcopy", &[]),
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
    fn survives_missing_artist_and_album() {
        let mut track = track();
        track.artist = String::new();
        track.album = String::new();
        assert_eq!(share_line(&track), "▶ Get Lucky · mfe-player");
    }
}
