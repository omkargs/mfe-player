//! Windows media integration through SMTC.
//!
//! A silent `MediaPlayer` owns a `SystemMediaTransportControls` session, so
//! the track shows up in the volume flyout, on the lock screen and in the
//! media-key overlay. The buttons come back as [`Command`]s, exactly like
//! MPRIS does on Linux.
//!
//! Everything runs on a dedicated thread with a multi-threaded apartment, so
//! the TUI never waits for WinRT and the main thread keeps its own COM state.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use windows::core::HSTRING;
use windows::Foundation::{TimeSpan, TypedEventHandler, Uri};
use windows::Media::Playback::MediaPlayer;
use windows::Media::{
    AutoRepeatModeChangeRequestedEventArgs, MediaPlaybackAutoRepeatMode, MediaPlaybackStatus,
    MediaPlaybackType, PlaybackPositionChangeRequestedEventArgs,
    ShuffleEnabledChangeRequestedEventArgs, SystemMediaTransportControls,
    SystemMediaTransportControlsButton, SystemMediaTransportControlsButtonPressedEventArgs,
    SystemMediaTransportControlsTimelineProperties,
};
use windows::Storage::Streams::RandomAccessStreamReference;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

use crate::platform::media::{Command, Update};

pub struct Smtc {
    pub commands: Receiver<Command>,
    updates: SyncSender<Update>,
    active: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
    _thread: Option<JoinHandle<()>>,
}

impl Smtc {
    pub fn start() -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let (update_tx, update_rx) = mpsc::sync_channel(1);
        let active = Arc::new(AtomicBool::new(false));
        let error = Arc::new(Mutex::new(None));

        let thread = thread::Builder::new()
            .name("smtc".to_string())
            .spawn({
                let active = Arc::clone(&active);
                let error = Arc::clone(&error);
                move || run(command_tx, update_rx, active, error)
            })
            .ok();

        Self {
            commands: command_rx,
            updates: update_tx,
            active,
            error,
            _thread: thread,
        }
    }

    /// Queue the current state for the OS. Never blocks: when the backend is
    /// busy the newest state replaces the queued one.
    pub fn push(&self, update: Update) {
        let _ = self.updates.try_send(update);
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    pub fn error(&self) -> Option<String> {
        self.error.lock().ok().and_then(|guard| guard.clone())
    }
}

fn run(
    commands: Sender<Command>,
    updates: Receiver<Update>,
    active: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
) {
    // A multi-threaded apartment is enough for the WinRT APIs we touch, and it
    // keeps the TUI thread free of COM state.
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }

    let player = match MediaPlayer::new() {
        Ok(player) => player,
        Err(err) => {
            set_error(&error, format!("smtc unavailable: {err}"));
            return;
        }
    };
    let smtc = match player.SystemMediaTransportControls() {
        Ok(smtc) => smtc,
        Err(err) => {
            set_error(&error, format!("smtc unavailable: {err}"));
            return;
        }
    };
    // The command manager is what actually publishes the session to the
    // system; the controls on their own only carry the metadata.
    if let Ok(manager) = player.CommandManager() {
        let _ = manager.SetIsEnabled(true);
    }
    if let Err(err) = smtc.SetIsEnabled(true) {
        set_error(&error, format!("smtc could not be enabled: {err}"));
        return;
    }

    register_handlers(&smtc, &commands);
    active.store(true, Ordering::SeqCst);

    let mut last: Option<Update> = None;
    let mut last_applied = std::time::Instant::now() - Duration::from_secs(10);
    loop {
        match updates.recv_timeout(Duration::from_millis(200)) {
            Ok(update) => {
                // The position moves on every tick; the OS does not need that
                // often. Anything else (track, pause, volume) goes straight
                // through.
                let changed = match &last {
                    None => true,
                    Some(previous) => {
                        previous.title != update.title
                            || previous.artist != update.artist
                            || previous.album != update.album
                            || previous.paused != update.paused
                            || previous.has_track != update.has_track
                            || previous.volume != update.volume
                            || previous.shuffle != update.shuffle
                            || previous.repeat != update.repeat
                            || previous.can_next != update.can_next
                            || previous.can_previous != update.can_previous
                            || (update.position_secs - previous.position_secs).abs() > 1.0
                    }
                };
                let stale = last_applied.elapsed() >= Duration::from_secs(2);
                if changed || stale {
                    apply(&smtc, &update);
                    last_applied = std::time::Instant::now();
                }
                last = Some(update);
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn register_handlers(smtc: &SystemMediaTransportControls, commands: &Sender<Command>) {
    {
        let tx = commands.clone();
        let _ =
            smtc.ButtonPressed(&TypedEventHandler::new(
                move |_sender,
                      args: windows::core::Ref<
                    SystemMediaTransportControlsButtonPressedEventArgs,
                >| {
                    let button = args
                        .as_ref()
                        .and_then(|args| args.Button().ok())
                        .unwrap_or(SystemMediaTransportControlsButton::Play);
                    let command = match button {
                        SystemMediaTransportControlsButton::Play => Command::Play,
                        SystemMediaTransportControlsButton::Pause => Command::Pause,
                        SystemMediaTransportControlsButton::Stop => Command::Stop,
                        SystemMediaTransportControlsButton::Next => Command::Next,
                        SystemMediaTransportControlsButton::Previous => Command::Previous,
                        SystemMediaTransportControlsButton::FastForward => Command::Seek(10.0),
                        SystemMediaTransportControlsButton::Rewind => Command::Seek(-10.0),
                        _ => Command::PlayPause,
                    };
                    let _ = tx.send(command);
                    Ok(())
                },
            ));
    }
    {
        let tx = commands.clone();
        let _ = smtc.PlaybackPositionChangeRequested(&TypedEventHandler::new(
            move |_sender, args: windows::core::Ref<PlaybackPositionChangeRequestedEventArgs>| {
                if let Some(args) = args.as_ref() {
                    if let Ok(position) = args.RequestedPlaybackPosition() {
                        let _ = tx.send(Command::SetPosition(
                            position.Duration as f64 / 10_000_000.0,
                        ));
                    }
                }
                Ok(())
            },
        ));
    }
    {
        let tx = commands.clone();
        let _ = smtc.ShuffleEnabledChangeRequested(&TypedEventHandler::new(
            move |_sender, args: windows::core::Ref<ShuffleEnabledChangeRequestedEventArgs>| {
                if let Some(args) = args.as_ref() {
                    if let Ok(shuffle) = args.RequestedShuffleEnabled() {
                        let _ = tx.send(Command::SetShuffle(shuffle));
                    }
                }
                Ok(())
            },
        ));
    }
    {
        let tx = commands.clone();
        let _ = smtc.AutoRepeatModeChangeRequested(&TypedEventHandler::new(
            move |_sender, args: windows::core::Ref<AutoRepeatModeChangeRequestedEventArgs>| {
                if let Some(args) = args.as_ref() {
                    if let Ok(mode) = args.RequestedAutoRepeatMode() {
                        let label = match mode {
                            MediaPlaybackAutoRepeatMode::Track => "one",
                            MediaPlaybackAutoRepeatMode::List => "all",
                            _ => "off",
                        };
                        let _ = tx.send(Command::SetLoop(label.to_string()));
                    }
                }
                Ok(())
            },
        ));
    }
}

fn apply(smtc: &SystemMediaTransportControls, update: &Update) {
    let status = if !update.has_track {
        MediaPlaybackStatus::Stopped
    } else if update.paused {
        MediaPlaybackStatus::Paused
    } else {
        MediaPlaybackStatus::Playing
    };
    let _ = smtc.SetPlaybackStatus(status);
    let _ = smtc.SetPlaybackRate(if update.paused { 0.0 } else { 1.0 });
    let _ = smtc.SetIsPlayEnabled(true);
    let _ = smtc.SetIsPauseEnabled(true);
    let _ = smtc.SetIsStopEnabled(true);
    let _ = smtc.SetIsNextEnabled(update.can_next);
    let _ = smtc.SetIsPreviousEnabled(update.can_previous);
    let _ = smtc.SetShuffleEnabled(update.shuffle);
    let _ = smtc.SetAutoRepeatMode(match update.repeat.as_str() {
        "one" => MediaPlaybackAutoRepeatMode::Track,
        "all" => MediaPlaybackAutoRepeatMode::List,
        _ => MediaPlaybackAutoRepeatMode::None,
    });

    if let Ok(updater) = smtc.DisplayUpdater() {
        let _ = updater.SetType(MediaPlaybackType::Music);
        if let Ok(properties) = updater.MusicProperties() {
            let title = if update.title.is_empty() {
                "Nothing playing"
            } else {
                update.title.as_str()
            };
            let artist = if update.artist.is_empty() {
                "mfe-player"
            } else {
                update.artist.as_str()
            };
            let _ = properties.SetTitle(&HSTRING::from(title));
            let _ = properties.SetArtist(&HSTRING::from(artist));
            let _ = properties.SetAlbumTitle(&HSTRING::from(update.album.as_str()));
        }
        // Album art, shown in the volume flyout and on the lock screen. A
        // local file path is not a URI the OS can fetch, so only remote art
        // goes up; passing `None` clears the previous cover.
        let thumbnail = update
            .art_url
            .as_deref()
            .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
            .and_then(|url| Uri::CreateUri(&HSTRING::from(url)).ok())
            .and_then(|uri| RandomAccessStreamReference::CreateFromUri(&uri).ok());
        let _ = updater.SetThumbnail(thumbnail.as_ref());
        let _ = updater.Update();
    }

    let duration = (update.duration_secs.max(0.0) * 10_000_000.0) as i64;
    let position = (update.position_secs.max(0.0) * 10_000_000.0) as i64;
    if let Ok(timeline) = SystemMediaTransportControlsTimelineProperties::new() {
        let _ = timeline.SetStartTime(TimeSpan { Duration: 0 });
        let _ = timeline.SetEndTime(TimeSpan { Duration: duration });
        let _ = timeline.SetMinSeekTime(TimeSpan { Duration: 0 });
        let _ = timeline.SetMaxSeekTime(TimeSpan { Duration: duration });
        let _ = timeline.SetPosition(TimeSpan { Duration: position });
        let _ = smtc.UpdateTimelineProperties(&timeline);
    }
}

fn set_error(error: &Arc<Mutex<Option<String>>>, message: String) {
    if let Ok(mut guard) = error.lock() {
        *guard = Some(message);
    }
}
