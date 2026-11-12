//! macOS media integration through `MPNowPlayingInfoCenter` and
//! `MPRemoteCommandCenter`.
//!
//! Pushes metadata and playback state to Control Center, the lock screen and
//! the media keys. Remote key presses come back as [`Command`]s on the same
//! channel every other backend uses.
//!
//! Both centers are process-wide singletons and the handlers are registered
//! once per thread, so this backend only pushes when the metadata changes.

use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::AnyObject;
use objc2_foundation::{NSDictionary, NSNumber, NSString};
use objc2_media_player::{
    MPChangePlaybackPositionCommand, MPChangePlaybackPositionCommandEvent,
    MPMediaItemPropertyAlbumTitle, MPMediaItemPropertyArtist, MPMediaItemPropertyPlaybackDuration,
    MPMediaItemPropertyTitle, MPNowPlayingInfoCenter, MPNowPlayingInfoPropertyElapsedPlaybackTime,
    MPNowPlayingInfoPropertyPlaybackRate, MPNowPlayingPlaybackState, MPRemoteCommand,
    MPRemoteCommandCenter, MPRemoteCommandEvent, MPRemoteCommandHandlerStatus,
};

use crate::platform::media::{Command, Update};

/// The shape `MPRemoteCommandCenter` expects a handler to have.
type Handler = RcBlock<dyn Fn(NonNull<MPRemoteCommandEvent>) -> MPRemoteCommandHandlerStatus>;

pub struct NowPlaying {
    pub commands: Receiver<Command>,
    updates: SyncSender<Update>,
    active: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
    _thread: Option<JoinHandle<()>>,
}

impl NowPlaying {
    pub fn start() -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let (update_tx, update_rx) = mpsc::sync_channel(1);
        let active = Arc::new(AtomicBool::new(false));
        let error = Arc::new(Mutex::new(None));

        let spawned = thread::Builder::new()
            .name("now-playing".to_string())
            .spawn({
                let active = Arc::clone(&active);
                move || run(command_tx, update_rx, active)
            });

        let thread = match spawned {
            Ok(thread) => Some(thread),
            Err(err) => {
                set_error(&error, format!("could not start the media thread: {err}"));
                None
            }
        };

        Self {
            commands: command_rx,
            updates: update_tx,
            active,
            error,
            _thread: thread,
        }
    }

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

fn run(commands: Sender<Command>, updates: Receiver<Update>, active: Arc<AtomicBool>) {
    autoreleasepool(|_| {
        // SAFETY: both are process-wide singletons handed out by the system,
        // and neither can fail on a supported macOS release.
        let center = unsafe { MPNowPlayingInfoCenter::defaultCenter() };
        let remote = unsafe { MPRemoteCommandCenter::sharedCommandCenter() };

        // The system retains whatever is registered with it; keeping our own
        // handles documents that registration has to outlive the loop below.
        let mut registered: Vec<(Retained<AnyObject>, Handler)> = Vec::new();
        register_commands(&remote, &commands, &mut registered);
        active.store(true, Ordering::SeqCst);

        let mut last: Option<Update> = None;
        let mut last_applied = Instant::now() - Duration::from_secs(10);

        loop {
            match updates.recv_timeout(Duration::from_millis(200)) {
                Ok(update) => {
                    let changed = match &last {
                        None => true,
                        Some(prev) => {
                            prev.title != update.title
                                || prev.artist != update.artist
                                || prev.album != update.album
                                || prev.paused != update.paused
                                || prev.has_track != update.has_track
                                || prev.shuffle != update.shuffle
                                || prev.repeat != update.repeat
                                || (update.position_secs - prev.position_secs).abs() > 1.0
                        }
                    };
                    // Re-publish on a timer as well: macOS forgets the elapsed
                    // time it shows if nothing refreshes it for a while.
                    let stale = last_applied.elapsed() >= Duration::from_secs(2);
                    if changed || stale {
                        apply_update(&center, &update);
                        last_applied = Instant::now();
                    }
                    last = Some(update);
                }
                Err(RecvTimeoutError::Timeout) => continue,
                // The TUI is gone. Clear the lock screen only when nothing
                // else is playing: after an external close the daemon owns
                // playback, and wiping the entry here would erase the
                // metadata it just published.
                Err(RecvTimeoutError::Disconnected) => {
                    if !crate::daemon::is_running() {
                        unsafe {
                            center.setNowPlayingInfo(None);
                            center.setPlaybackState(MPNowPlayingPlaybackState::Stopped);
                        }
                    }
                    active.store(false, Ordering::SeqCst);
                    break;
                }
            }
        }

        // Registration must stay alive until the loop above is finished.
        drop(registered);
    });
}

fn register_commands(
    remote: &MPRemoteCommandCenter,
    commands: &Sender<Command>,
    registered: &mut Vec<(Retained<AnyObject>, Handler)>,
) {
    unsafe {
        // SAFETY: every accessor returns a command owned by the system.
        attach(&remote.playCommand(), Command::Play, commands, registered);
        attach(&remote.pauseCommand(), Command::Pause, commands, registered);
        attach(&remote.stopCommand(), Command::Stop, commands, registered);
        attach(
            &remote.togglePlayPauseCommand(),
            Command::PlayPause,
            commands,
            registered,
        );
        attach(
            &remote.nextTrackCommand(),
            Command::Next,
            commands,
            registered,
        );
        attach(
            &remote.previousTrackCommand(),
            Command::Previous,
            commands,
            registered,
        );
        seek_command(
            &remote.changePlaybackPositionCommand(),
            commands,
            registered,
        );
    }
}

/// Enables `command` and forwards every press to the app.
fn attach(
    command: &MPRemoteCommand,
    kind: Command,
    commands: &Sender<Command>,
    registered: &mut Vec<(Retained<AnyObject>, Handler)>,
) {
    let tx = commands.clone();
    let handler: Handler = RcBlock::new(move |_event: NonNull<MPRemoteCommandEvent>| {
        let _ = tx.send(kind.clone());
        MPRemoteCommandHandlerStatus::Success
    });
    let target = unsafe {
        command.setEnabled(true);
        command.addTargetWithHandler(&handler)
    };
    registered.push((target, handler));
}

/// Scrubbing arrives as a dedicated event carrying the absolute position.
fn seek_command(
    command: &MPChangePlaybackPositionCommand,
    commands: &Sender<Command>,
    registered: &mut Vec<(Retained<AnyObject>, Handler)>,
) {
    let tx = commands.clone();
    let handler: Handler = RcBlock::new(move |event: NonNull<MPRemoteCommandEvent>| {
        // SAFETY: this handler is only ever invoked with a
        // `MPChangePlaybackPositionCommandEvent`, which is what the cast
        // assumes; `positionTime` is a plain `f64` getter.
        let position = unsafe {
            (*event
                .as_ptr()
                .cast::<MPChangePlaybackPositionCommandEvent>())
            .positionTime()
        };
        let _ = tx.send(Command::SetPosition(position));
        MPRemoteCommandHandlerStatus::Success
    });
    let target = unsafe {
        command.setEnabled(true);
        command.addTargetWithHandler(&handler)
    };
    registered.push((target, handler));
}

fn apply_update(center: &MPNowPlayingInfoCenter, update: &Update) {
    autoreleasepool(|_| {
        let title = if update.title.is_empty() {
            NSString::from_str("Nothing playing")
        } else {
            NSString::from_str(&update.title)
        };
        let artist = if update.artist.is_empty() {
            NSString::from_str("mfe-player")
        } else {
            NSString::from_str(&update.artist)
        };
        let album = NSString::from_str(&update.album);

        let duration = NSNumber::new_f64(update.duration_secs.max(0.0));
        let position = NSNumber::new_f64(update.position_secs.max(0.0));
        let rate = NSNumber::new_f64(if update.paused { 0.0 } else { 1.0 });

        // SAFETY: these are link-time constants of the MediaPlayer framework.
        let keys: &[&NSString] = &[
            unsafe { MPMediaItemPropertyTitle },
            unsafe { MPMediaItemPropertyArtist },
            unsafe { MPMediaItemPropertyAlbumTitle },
            unsafe { MPMediaItemPropertyPlaybackDuration },
            unsafe { MPNowPlayingInfoPropertyElapsedPlaybackTime },
            unsafe { MPNowPlayingInfoPropertyPlaybackRate },
        ];
        let objects: &[&AnyObject] = &[&title, &artist, &album, &duration, &position, &rate];

        let info = NSDictionary::from_slices(keys, objects);
        unsafe {
            center.setNowPlayingInfo(Some(&info));
            // Unlike iOS, macOS cannot infer playback state from an audio
            // session, so the state has to ride along with the metadata or
            // the media keys stop working.
            center.setPlaybackState(if !update.has_track {
                MPNowPlayingPlaybackState::Stopped
            } else if update.paused {
                MPNowPlayingPlaybackState::Paused
            } else {
                MPNowPlayingPlaybackState::Playing
            });
        }
    });
}

fn set_error(error: &Arc<Mutex<Option<String>>>, message: String) {
    if let Ok(mut guard) = error.lock() {
        *guard = Some(message);
    }
}
