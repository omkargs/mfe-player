use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use async_channel::{Receiver as AsyncReceiver, Sender as AsyncSender};
use mpris_server::zbus;
use mpris_server::{LoopStatus, Metadata, PlaybackStatus, Player, Time, TrackId};

pub use crate::platform::media::{Command, Update};

pub struct Mpris {
    pub commands: Receiver<Command>,
    updates: AsyncSender<Update>,
    active: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
    _thread: thread::JoinHandle<()>,
}

impl Mpris {
    pub fn start() -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        // Metadata updates are snapshots, not events: if the D-Bus loop is
        // busy, keep the newest and drop older ones instead of queueing
        // forever. Unbounded here would grow with every track change.
        let (update_tx, update_rx) = async_channel::bounded(64);
        let active = Arc::new(AtomicBool::new(false));
        let error = Arc::new(Mutex::new(None));

        let thread = thread::Builder::new()
            .name("mpris".to_string())
            .spawn({
                let active = Arc::clone(&active);
                let error = Arc::clone(&error);
                move || run_server(command_tx, update_rx, active, error)
            })
            .expect("starting MPRIS thread");

        Self {
            commands: command_rx,
            updates: update_tx,
            active,
            error,
            _thread: thread,
        }
    }

    pub fn push(&self, update: Update) {
        // Keeps the queue bounded: the oldest snapshot is dropped when the
        // D-Bus loop falls behind, so the desktop always sees the latest.
        let _ = self.updates.force_send(update);
    }

    /// True once the D-Bus name is owned, i.e. the desktop shows our card.
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    /// Why the card is missing, if it is.
    pub fn error(&self) -> Option<String> {
        self.error.lock().ok().and_then(|guard| guard.clone())
    }
}

fn run_server(
    commands: Sender<Command>,
    updates: AsyncReceiver<Update>,
    active: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
) {
    zbus::block_on(async move {
        let player = match Player::builder("org.mfeplayer.MfePlayer")
            .identity("Mfe Player")
            .desktop_entry("mfe-player")
            .can_quit(true)
            .can_raise(false)
            .can_play(true)
            .can_pause(true)
            .can_go_next(true)
            .can_go_previous(true)
            .can_seek(true)
            .can_control(true)
            .build()
            .await
        {
            Ok(player) => player,
            // No session bus (headless, ssh without DBus): MPRIS is optional.
            Err(err) => {
                if let Ok(mut guard) = error.lock() {
                    *guard = Some(format!("mpris unavailable: {err}"));
                }
                return;
            }
        };
        active.store(true, Ordering::SeqCst);

        {
            let tx = commands.clone();
            player.connect_quit(move |_| {
                let _ = tx.send(Command::Quit);
            });
        }
        {
            let tx = commands.clone();
            player.connect_play(move |_| {
                let _ = tx.send(Command::Play);
            });
            let tx = commands.clone();
            player.connect_pause(move |_| {
                let _ = tx.send(Command::Pause);
            });
            let tx = commands.clone();
            player.connect_play_pause(move |_| {
                let _ = tx.send(Command::PlayPause);
            });
            let tx = commands.clone();
            player.connect_stop(move |_| {
                let _ = tx.send(Command::Stop);
            });
            let tx = commands.clone();
            player.connect_next(move |_| {
                let _ = tx.send(Command::Next);
            });
            let tx = commands.clone();
            player.connect_previous(move |_| {
                let _ = tx.send(Command::Previous);
            });
        }
        {
            let tx = commands.clone();
            player.connect_seek(move |_, offset| {
                let _ = tx.send(Command::Seek(offset.as_micros() as f64 / 1_000_000.0));
            });
            let tx = commands.clone();
            player.connect_set_position(move |_, _track, position| {
                let _ = tx.send(Command::SetPosition(
                    position.as_micros() as f64 / 1_000_000.0,
                ));
            });
            let tx = commands.clone();
            player.connect_set_volume(move |_, volume| {
                let _ = tx.send(Command::SetVolume(volume * 100.0));
            });
            let tx = commands.clone();
            player.connect_set_shuffle(move |_, shuffle| {
                let _ = tx.send(Command::SetShuffle(shuffle));
            });
            let tx = commands.clone();
            player.connect_set_loop_status(move |_, status| {
                let _ = tx.send(Command::SetLoop(loop_label(status).to_string()));
            });
        }

        let update_loop = async {
            while let Ok(update) = updates.recv().await {
                let status = if !update.has_track {
                    PlaybackStatus::Stopped
                } else if update.paused {
                    PlaybackStatus::Paused
                } else {
                    PlaybackStatus::Playing
                };
                let _ = player.set_playback_status(status).await;
                let track_id = TrackId::try_from("/org/mfeplayer/MfePlayer/track").ok();
                let mut metadata = Metadata::builder()
                    .title(if update.title.is_empty() {
                        "Nothing playing".to_string()
                    } else {
                        update.title.clone()
                    })
                    .album(update.album.clone())
                    .artist([if update.artist.is_empty() {
                        "mfe-player".to_string()
                    } else {
                        update.artist.clone()
                    }]);
                if let Some(track_id) = track_id {
                    metadata = metadata.trackid(track_id);
                }
                if update.duration_secs > 0.0 {
                    metadata = metadata.length(Time::from_micros(
                        (update.duration_secs * 1_000_000.0) as i64,
                    ));
                }
                if let Some(art) = update
                    .art_url
                    .clone()
                    .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
                {
                    metadata = metadata.art_url(art);
                }
                let _ = player.set_metadata(metadata.build()).await;
                player.set_position(Time::from_micros(
                    (update.position_secs.max(0.0) * 1_000_000.0) as i64,
                ));
                let _ = player
                    .set_volume((update.volume / 100.0).clamp(0.0, 1.5))
                    .await;
                let _ = player.set_shuffle(update.shuffle).await;
                let _ = player.set_loop_status(loop_status(&update.repeat)).await;
                let _ = player.set_can_go_next(update.can_next).await;
                let _ = player.set_can_go_previous(update.can_previous).await;
            }
        };

        let _ = futures_lite::future::zip(player.run(), update_loop).await;
    });
}

fn loop_status(label: &str) -> LoopStatus {
    match label {
        "one" => LoopStatus::Track,
        "all" => LoopStatus::Playlist,
        _ => LoopStatus::None,
    }
}

fn loop_label(status: LoopStatus) -> &'static str {
    match status {
        LoopStatus::Track => "one",
        LoopStatus::Playlist => "all",
        LoopStatus::None => "off",
    }
}
