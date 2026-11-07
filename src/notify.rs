//! Opt-in desktop notifications when a new track starts.
//!
//! Every platform ships its own notifier, so this is three small `Command`
//! calls rather than four crates and a runtime. Calls are spawned on a
//! throwaway thread and never block the UI; a missing notifier is silently
//! ignored because "no toast" is not a playback error.

use std::process::{Command, Stdio};
use std::thread;

use crate::models::Track;

/// Fire and forget. Safe to call on every track change.
pub fn now_playing(track: &Track) {
    let title = track.title.clone();
    let body = if track.album.is_empty() {
        track.artist.clone()
    } else {
        format!("{} — {}", track.artist, track.album)
    };
    let _ = thread::Builder::new()
        .name("notify".to_string())
        .spawn(move || send(&title, &body));
}

#[cfg(target_os = "linux")]
fn send(title: &str, body: &str) {
    let _ = Command::new("notify-send")
        .args(["-a", "mfe-player", "-c", "player", title, body])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

#[cfg(target_os = "macos")]
fn send(title: &str, body: &str) {
    let script = "on run argv
display notification (item 2 of argv) with title (item 1 of argv)
end run";
    let _ = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .arg("--")
        .arg(title)
        .arg(body)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

#[cfg(windows)]
fn send(title: &str, body: &str) {
    // Windows 10/11 toast through the built-in WinRT API. Values arrive as
    // environment variables so PowerShell never has to quote user text.
    let script = r#"
$ErrorActionPreference = 'SilentlyContinue'
[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null
$xml = New-Object Windows.Data.Xml.Dom.XmlDocument
$t = [System.Security.SecurityElement]::Escape($env:MFE_NOTIFY_TITLE)
$b = [System.Security.SecurityElement]::Escape($env:MFE_NOTIFY_BODY)
$xml.LoadXml("<toast><visual><binding template=""ToastGeneric""><text>$t</text><text>$b</text></binding></visual></toast>")
$toast = [Windows.UI.Notifications.ToastNotification]::new($xml)
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('mfe-player').Show($toast)
"#;
    let _ = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("MFE_NOTIFY_TITLE", title)
        .env("MFE_NOTIFY_BODY", body)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn send(_title: &str, _body: &str) {}
