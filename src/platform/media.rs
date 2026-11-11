//! Media integration exposed to the rest of the app.
//!
//! Linux uses MPRIS over D-Bus (media keys, `playerctl`, desktop widgets).
//! Other platforms get an inert implementation until their native layer lands
//! (SMTC on Windows, MPNowPlayingInfoCenter on macOS), so the rest of the app
//! does not have to care which one is present.

#[cfg(target_os = "linux")]
pub use crate::mpris::{Command, Mpris, Update};

#[cfg(not(target_os = "linux"))]
pub use stub::{Command, Mpris, Update};

#[cfg(not(target_os = "linux"))]
mod stub {
    use std::sync::mpsc::{self, Receiver};

    /// Commands arriving from OS media controls. Empty on platforms whose
    /// native layer is not wired up yet.
    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    pub enum Command {
        Play,
        Pause,
        PlayPause,
        Next,
        Previous,
        Stop,
        Seek(f64),
        SetPosition(f64),
        SetVolume(f64),
        SetShuffle(bool),
        SetLoop(String),
        Quit,
    }

    /// Snapshot pushed from the TUI to the media layer.
    #[derive(Debug, Clone, Default)]
    #[allow(dead_code)]
    pub struct Update {
        pub title: String,
        pub artist: String,
        pub album: String,
        pub art_url: Option<String>,
        pub duration_secs: f64,
        pub position_secs: f64,
        pub paused: bool,
        pub has_track: bool,
        pub volume: f64,
        pub can_next: bool,
        pub can_previous: bool,
        pub shuffle: bool,
        pub repeat: String,
    }

    /// No-op media server. Keeps the same shape as the Linux one so callers
    /// never need their own `cfg` blocks.
    pub struct Mpris {
        pub commands: Receiver<Command>,
    }

    impl Mpris {
        pub fn start() -> Self {
            let (_tx, rx) = mpsc::channel();
            Self { commands: rx }
        }

        pub fn push(&self, _update: Update) {}
    }
}
