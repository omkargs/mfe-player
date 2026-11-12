//! Media integration exposed to the rest of the app.
//!
//! One API, three backends:
//!
//! * Linux — MPRIS over D-Bus (media keys, `playerctl`, desktop widgets).
//! * Windows — SMTC through the `SystemMediaTransportControls` of a silent
//!   `MediaPlayer`, so the OS volume flyout, lock screen and media keys work.
//! * macOS — `MPNowPlayingInfoCenter` plus `MPRemoteCommandCenter`.
//!
//! Every backend speaks the same [`Command`]/[`Update`] pair, so the rest of
//! the app never needs its own `cfg` blocks. A backend that cannot start
//! (no session bus, missing framework, locked-down desktop) degrades to
//! silence: playback continues, the media card simply does not appear.

/// Commands arriving from OS media controls.
///
/// Not every desktop exposes every command: MPRIS has quit and volume, while
/// SMTC and Now Playing have neither. The variants stay so the app only ever
/// handles this one vocabulary, wherever the keystroke comes from.
#[allow(dead_code)]
#[derive(Debug, Clone)]
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
pub struct Update {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Cover art. MPRIS and SMTC take the URL as-is; Now Playing needs an
    /// `NSImage` object instead, so the macOS backend does not show it yet.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
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

/// The media backend that is active in this process.
pub struct Media {
    backend: Backend,
}

enum Backend {
    #[cfg(target_os = "linux")]
    Mpris(crate::mpris::Mpris),
    #[cfg(windows)]
    Smtc(crate::platform::windows_smtc::Smtc),
    #[cfg(target_os = "macos")]
    NowPlaying(crate::platform::macos_nowplaying::NowPlaying),
    Off,
}

impl Media {
    /// Start the platform backend. Never fails: an unavailable backend simply
    /// reports no commands and swallows updates.
    pub fn start() -> Self {
        #[cfg(target_os = "linux")]
        {
            Self {
                backend: Backend::Mpris(crate::mpris::Mpris::start()),
            }
        }
        #[cfg(windows)]
        {
            Self {
                backend: Backend::Smtc(crate::platform::windows_smtc::Smtc::start()),
            }
        }
        #[cfg(target_os = "macos")]
        {
            Self {
                backend: Backend::NowPlaying(crate::platform::macos_nowplaying::NowPlaying::start()),
            }
        }
        #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
        {
            Self {
                backend: Backend::Off,
            }
        }
    }

    /// A media layer that does nothing, used when the user turns it off.
    pub fn off() -> Self {
        Self {
            backend: Backend::Off,
        }
    }

    /// Drain the commands the OS sent since the last poll.
    pub fn commands(&self) -> Vec<Command> {
        match &self.backend {
            #[cfg(target_os = "linux")]
            Backend::Mpris(mpris) => mpris.commands.try_iter().collect(),
            #[cfg(windows)]
            Backend::Smtc(smtc) => smtc.commands.try_iter().collect(),
            #[cfg(target_os = "macos")]
            Backend::NowPlaying(now_playing) => now_playing.commands.try_iter().collect(),
            Backend::Off => Vec::new(),
        }
    }

    /// Push the current playback state to the OS.
    pub fn push(&self, update: Update) {
        match &self.backend {
            #[cfg(target_os = "linux")]
            Backend::Mpris(mpris) => mpris.push(update),
            #[cfg(windows)]
            Backend::Smtc(smtc) => smtc.push(update),
            #[cfg(target_os = "macos")]
            Backend::NowPlaying(now_playing) => now_playing.push(update),
            Backend::Off => {}
        }
    }

    /// Short name of the backend, for `mfe media` and the doctor.
    pub fn backend_name(&self) -> &'static str {
        match &self.backend {
            #[cfg(target_os = "linux")]
            Backend::Mpris(_) => "mpris",
            #[cfg(windows)]
            Backend::Smtc(_) => "smtc",
            #[cfg(target_os = "macos")]
            Backend::NowPlaying(_) => "now-playing",
            Backend::Off => "off",
        }
    }

    /// Whether the OS actually shows a media card for this process.
    pub fn is_active(&self) -> bool {
        match &self.backend {
            #[cfg(target_os = "linux")]
            Backend::Mpris(mpris) => mpris.is_active(),
            #[cfg(windows)]
            Backend::Smtc(smtc) => smtc.is_active(),
            #[cfg(target_os = "macos")]
            Backend::NowPlaying(now_playing) => now_playing.is_active(),
            Backend::Off => false,
        }
    }

    /// Last error the backend ran into, if any.
    pub fn error(&self) -> Option<String> {
        match &self.backend {
            #[cfg(target_os = "linux")]
            Backend::Mpris(mpris) => mpris.error(),
            #[cfg(windows)]
            Backend::Smtc(smtc) => smtc.error(),
            #[cfg(target_os = "macos")]
            Backend::NowPlaying(now_playing) => now_playing.error(),
            Backend::Off => None,
        }
    }
}

/// Backend compiled into this binary, without starting it. Used by
/// `mfe doctor`, which must not claim the media name just to describe it.
pub fn backend_name() -> &'static str {
    if cfg!(target_os = "linux") {
        "mpris"
    } else if cfg!(windows) {
        "smtc"
    } else if cfg!(target_os = "macos") {
        "now-playing"
    } else {
        "off"
    }
}

/// The commands every backend can deliver, listed for the CLI and the help
/// screen so the documentation cannot drift from the implementation.
pub const CAPABILITIES: &[&str] = &[
    "play",
    "pause",
    "play/pause",
    "next",
    "previous",
    "stop",
    "seek",
    "position",
    "volume",
    "shuffle",
    "repeat",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_media_never_speaks() {
        let media = Media::off();
        assert!(media.commands().is_empty());
        assert_eq!(media.backend_name(), "off");
        assert!(!media.is_active());
        media.push(Update::default());
    }
}
