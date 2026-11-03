use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use ratatui::layout::Rect;

use crate::artwork::Artwork;
use crate::audio::{AudioEngine, AudioSnapshot};
use crate::cava::Cava;
use crate::config::{self, AppConfig};
use crate::library;
use crate::models::{
    parse_lyrics, truncate, Album, Artist, LyricLine, LyricsDoc, Playlist, SearchPayload,
    SearchResults, StreamInfo, Track,
};
use crate::mpris::{self, Mpris};
use crate::plugins::{run_plugin, InstalledPlugin, PluginKind, PluginRegistry};
use crate::social::Scrobbler;
use crate::state::{HistoryEntry, PersistedState};
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Search,
    Queue,
    Favorites,
    Playlists,
    Downloads,
    Plugins,
    History,
    Library,
    Browse,
}

impl View {
    pub const ALL: [View; 8] = [
        View::Search,
        View::Queue,
        View::Favorites,
        View::Playlists,
        View::Downloads,
        View::Plugins,
        View::History,
        View::Library,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::Queue => "queue",
            Self::Favorites => "favorites",
            Self::Playlists => "playlists",
            Self::Downloads => "downloads",
            Self::Plugins => "plugin store",
            Self::History => "history",
            Self::Library => "local library",
            Self::Browse => "browse",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Search => "⌕",
            Self::Queue => "≡",
            Self::Favorites => "★",
            Self::Playlists => "▤",
            Self::Downloads => "⤓",
            Self::Plugins => "⚙",
            Self::History => "◷",
            Self::Library => "♫",
            Self::Browse => "◉",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchRow {
    Header(&'static str),
    Artist(usize),
    Album(usize),
    Track(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowseKind {
    Artist,
    Album,
}

#[derive(Debug, Clone)]
pub struct BrowsePage {
    pub kind: BrowseKind,
    pub title: String,
    pub subtitle: String,
    pub artwork: Option<String>,
    pub source: String,
    pub albums: Vec<Album>,
    pub tracks: Vec<Track>,
    pub selected: usize,
    pub loading: bool,
    pub error: Option<String>,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowseRow {
    Header(&'static str),
    Album(usize),
    Track(usize),
}

impl BrowsePage {
    pub fn kind_label(&self) -> &'static str {
        match self.kind {
            BrowseKind::Artist => "◉ artist · ",
            BrowseKind::Album => "▣ album · ",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatMode {
    Off,
    All,
    One,
}

impl RepeatMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "repeat off",
            Self::All => "repeat all",
            Self::One => "repeat one",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Off => Self::All,
            Self::All => Self::One,
            Self::One => Self::Off,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SleepMode {
    Off,
    EndOfTrack,
    At(Instant),
}

pub const SLEEP_OPTIONS: &[(&str, u32)] = &[
    ("off", 0),
    ("15 minutes", 15),
    ("30 minutes", 30),
    ("45 minutes", 45),
    ("60 minutes", 60),
    ("90 minutes", 90),
    ("end of current track", u32::MAX),
];

#[derive(Debug, Clone)]
pub struct LinkImport {
    pub title: String,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone)]
pub enum UndoAction {
    Queue {
        track: Track,
        index: usize,
    },
    Favorites {
        track: Track,
        index: usize,
    },
    Playlist {
        playlist: usize,
        track: Track,
        index: usize,
    },
}

/// A clickable rectangle recorded while drawing. Keeping the hit map in the
/// app means mouse support follows the real layout instead of guessing where
/// rows ended up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitAction {
    SwitchView(View),
    Select(View, usize),
    SeekBar,
}

#[derive(Debug, Clone, Copy)]
pub struct HitRegion {
    pub area: Rect,
    pub action: HitAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterKind {
    Search,
    Queue,
    Favorites,
    Playlist(usize),
    Library,
}

#[derive(Debug, Clone, Copy)]
pub enum ConfirmAction {
    DeletePlaylist(usize),
    ClearQueue,
}

#[derive(Debug, Clone)]
pub struct LyricsOverlay {
    pub track: Track,
    pub doc: Option<LyricsDoc>,
    pub lines: Vec<LyricLine>,
    pub loading: bool,
    pub error: Option<String>,
    pub scroll: usize,
    pub follow: bool,
}

#[derive(Debug)]
pub enum Overlay {
    Help,
    CommandPalette {
        query: String,
        selected: usize,
    },
    Filter {
        kind: FilterKind,
        query: String,
        selected: usize,
    },
    PlaylistPicker {
        selected: usize,
    },
    NewPlaylist {
        name: String,
    },
    RenamePlaylist {
        index: usize,
        name: String,
    },
    Confirm {
        message: String,
        action: ConfirmAction,
    },
    Sleep {
        selected: usize,
    },
    ImportLink {
        input: String,
    },
    Stats,
    Lyrics(Box<LyricsOverlay>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteAction {
    PlaySelected,
    QueueSelected,
    PlayPause,
    NextTrack,
    PreviousTrack,
    SeekForward,
    SeekBackward,
    SeekForwardLong,
    SeekBackwardLong,
    VolumeUp,
    VolumeDown,
    ToggleMute,
    ToggleFavorite,
    ShowLyrics,
    JumpToNowPlaying,
    PlayPage,
    FocusSearch,
    ViewSearch,
    ViewQueue,
    ViewFavorites,
    ViewPlaylists,
    ViewDownloads,
    ViewPlugins,
    ViewHistory,
    ViewLibrary,
    RescanLibrary,
    ShareNowPlaying,
    ShowStats,
    GoBack,
    PlayNext,
    RenamePlaylist,
    NewPlaylist,
    AddToPlaylist,
    ToggleShuffle,
    CycleRepeat,
    ClearQueue,
    ClearUpcoming,
    Undo,
    SaveQueue,
    ExportPlaylist,
    ImportPlaylist,
    ImportLink,
    DownloadSelected,
    ToggleRadio,
    SleepTimer,
    ToggleNormalize,
    CycleEq,
    CycleTheme,
    ToggleCava,
    Background,
    Help,
    Quit,
}

#[derive(Debug, Clone, Copy)]
pub struct PaletteItem {
    pub label: &'static str,
    pub hint: &'static str,
    pub action: PaletteAction,
}

pub const PALETTE_ITEMS: &[PaletteItem] = &[
    PaletteItem {
        label: "Play selected track",
        hint: "Enter",
        action: PaletteAction::PlaySelected,
    },
    PaletteItem {
        label: "Add selected to queue",
        hint: "a",
        action: PaletteAction::QueueSelected,
    },
    PaletteItem {
        label: "Play / pause",
        hint: "Space",
        action: PaletteAction::PlayPause,
    },
    PaletteItem {
        label: "Next track",
        hint: "n",
        action: PaletteAction::NextTrack,
    },
    PaletteItem {
        label: "Previous track",
        hint: "b",
        action: PaletteAction::PreviousTrack,
    },
    PaletteItem {
        label: "Seek forward 5s",
        hint: "→",
        action: PaletteAction::SeekForward,
    },
    PaletteItem {
        label: "Seek backward 5s",
        hint: "←",
        action: PaletteAction::SeekBackward,
    },
    PaletteItem {
        label: "Seek forward 30s",
        hint: "Shift+→",
        action: PaletteAction::SeekForwardLong,
    },
    PaletteItem {
        label: "Seek backward 30s",
        hint: "Shift+←",
        action: PaletteAction::SeekBackwardLong,
    },
    PaletteItem {
        label: "Volume up",
        hint: "+",
        action: PaletteAction::VolumeUp,
    },
    PaletteItem {
        label: "Volume down",
        hint: "-",
        action: PaletteAction::VolumeDown,
    },
    PaletteItem {
        label: "Mute / unmute",
        hint: "m",
        action: PaletteAction::ToggleMute,
    },
    PaletteItem {
        label: "Toggle favorite",
        hint: "F",
        action: PaletteAction::ToggleFavorite,
    },
    PaletteItem {
        label: "Show lyrics",
        hint: "y",
        action: PaletteAction::ShowLyrics,
    },
    PaletteItem {
        label: "Jump to current track in queue",
        hint: "t",
        action: PaletteAction::JumpToNowPlaying,
    },
    PaletteItem {
        label: "Play current album / artist page",
        hint: "p",
        action: PaletteAction::PlayPage,
    },
    PaletteItem {
        label: "Search music",
        hint: "/",
        action: PaletteAction::FocusSearch,
    },
    PaletteItem {
        label: "Go to search",
        hint: "1",
        action: PaletteAction::ViewSearch,
    },
    PaletteItem {
        label: "Go to queue",
        hint: "2",
        action: PaletteAction::ViewQueue,
    },
    PaletteItem {
        label: "Go to favorites",
        hint: "3",
        action: PaletteAction::ViewFavorites,
    },
    PaletteItem {
        label: "Go to playlists",
        hint: "4",
        action: PaletteAction::ViewPlaylists,
    },
    PaletteItem {
        label: "Go to plugin store",
        hint: "6",
        action: PaletteAction::ViewPlugins,
    },
    PaletteItem {
        label: "Go back (page / playlist)",
        hint: "h",
        action: PaletteAction::GoBack,
    },
    PaletteItem {
        label: "Play selected track next",
        hint: "N",
        action: PaletteAction::PlayNext,
    },
    PaletteItem {
        label: "Create playlist",
        hint: "c",
        action: PaletteAction::NewPlaylist,
    },
    PaletteItem {
        label: "Rename playlist",
        hint: "e",
        action: PaletteAction::RenamePlaylist,
    },
    PaletteItem {
        label: "Add selected to playlist",
        hint: "P",
        action: PaletteAction::AddToPlaylist,
    },
    PaletteItem {
        label: "Toggle shuffle",
        hint: "s",
        action: PaletteAction::ToggleShuffle,
    },
    PaletteItem {
        label: "Cycle repeat mode",
        hint: "r",
        action: PaletteAction::CycleRepeat,
    },
    PaletteItem {
        label: "Clear queue",
        hint: "C",
        action: PaletteAction::ClearQueue,
    },
    PaletteItem {
        label: "Clear upcoming tracks",
        hint: "Alt+K",
        action: PaletteAction::ClearUpcoming,
    },
    PaletteItem {
        label: "Undo last removal",
        hint: "Alt+U",
        action: PaletteAction::Undo,
    },
    PaletteItem {
        label: "Save queue as playlist",
        hint: "S",
        action: PaletteAction::SaveQueue,
    },
    PaletteItem {
        label: "Export playlists to M3U",
        hint: "Alt+E",
        action: PaletteAction::ExportPlaylist,
    },
    PaletteItem {
        label: "Import playlists from M3U",
        hint: "Alt+I",
        action: PaletteAction::ImportPlaylist,
    },
    PaletteItem {
        label: "Download selected for offline",
        hint: "d",
        action: PaletteAction::DownloadSelected,
    },
    PaletteItem {
        label: "Go to downloads",
        hint: "5",
        action: PaletteAction::ViewDownloads,
    },
    PaletteItem {
        label: "Go to listening history",
        hint: "7",
        action: PaletteAction::ViewHistory,
    },
    PaletteItem {
        label: "Go to local library",
        hint: "8",
        action: PaletteAction::ViewLibrary,
    },
    PaletteItem {
        label: "Rescan local library",
        hint: "R",
        action: PaletteAction::RescanLibrary,
    },
    PaletteItem {
        label: "Share now playing",
        hint: "Y",
        action: PaletteAction::ShareNowPlaying,
    },
    PaletteItem {
        label: "Listening stats",
        hint: "Ctrl+G",
        action: PaletteAction::ShowStats,
    },
    PaletteItem {
        label: "Toggle radio autoplay",
        hint: "Ctrl+T",
        action: PaletteAction::ToggleRadio,
    },
    PaletteItem {
        label: "Sleep timer",
        hint: "z",
        action: PaletteAction::SleepTimer,
    },
    PaletteItem {
        label: "Toggle loudness normalisation",
        hint: "Alt+N",
        action: PaletteAction::ToggleNormalize,
    },
    PaletteItem {
        label: "Cycle equaliser preset",
        hint: "E",
        action: PaletteAction::CycleEq,
    },
    PaletteItem {
        label: "Cycle colour theme",
        hint: "Alt+T",
        action: PaletteAction::CycleTheme,
    },
    PaletteItem {
        label: "Import songs or a playlist from a link",
        hint: "Ctrl+L",
        action: PaletteAction::ImportLink,
    },
    PaletteItem {
        label: "Toggle cava visualiser",
        hint: "Ctrl+V",
        action: PaletteAction::ToggleCava,
    },
    PaletteItem {
        label: "Keep playing in background",
        hint: "Ctrl+B",
        action: PaletteAction::Background,
    },
    PaletteItem {
        label: "Show keyboard help",
        hint: "?",
        action: PaletteAction::Help,
    },
    PaletteItem {
        label: "Quit and stop audio",
        hint: "q",
        action: PaletteAction::Quit,
    },
];

#[derive(Debug, Clone)]
pub struct PluginEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub kind: PluginKind,
    pub description: String,
    pub installed: bool,
    pub active: bool,
    pub bundled: bool,
}

enum AppEvent {
    SearchDone {
        generation: u64,
        cache_key: String,
        result: std::result::Result<SearchResults, String>,
    },
    BrowseDone {
        generation: u64,
        result: std::result::Result<SearchResults, String>,
    },
    StreamResolved {
        track_id: String,
        result: std::result::Result<StreamInfo, String>,
    },
    PrefetchDone {
        track_id: String,
        result: std::result::Result<StreamInfo, String>,
    },
    RadioDone {
        tracks: Vec<Track>,
    },
    LinkImportDone {
        result: std::result::Result<LinkImport, String>,
    },
    DownloadDone {
        track: Track,
        path: PathBuf,
        result: std::result::Result<(), String>,
    },
    LyricsDone {
        track_id: String,
        result: std::result::Result<LyricsDoc, String>,
    },
    ShareDone {
        result: std::result::Result<String, String>,
    },
    ArtworkDone {
        track_id: String,
        result: std::result::Result<Artwork, String>,
    },
    LibraryDone {
        tracks: Vec<Track>,
        error: Option<String>,
    },
}

pub struct App {
    pub view: View,
    pub config: AppConfig,
    pub theme_name: String,
    pub should_quit: bool,
    pub detached_on_quit: bool,
    pub search_query: String,
    pub search_focused: bool,
    pub search_results: SearchResults,
    pub search_selected: usize,
    pub search_loading: bool,
    pub search_error: Option<String>,
    search_dirty: bool,
    search_generation: u64,
    last_search_edit: Instant,
    search_cache: HashMap<String, (SearchResults, Instant)>,
    pub browse_stack: Vec<BrowsePage>,
    browse_generation: u64,
    pub queue: Vec<Track>,
    pub queue_selected: usize,
    pub favorites: Vec<Track>,
    pub favorites_selected: usize,
    pub history: Vec<HistoryEntry>,
    pub history_selected: usize,
    pub library: Vec<Track>,
    pub library_selected: usize,
    pub library_scanning: bool,
    library_paths: Vec<PathBuf>,
    pub playlists: Vec<Playlist>,
    pub playlist_selected: usize,
    pub downloads: HashMap<String, (Track, PathBuf)>,
    pub downloads_selected: usize,
    download_pending: HashSet<String>,
    pub cava: Option<Cava>,
    pub cava_enabled: bool,
    pub sleep: SleepMode,
    pub eq: String,
    pub normalize: bool,
    pub radio: bool,
    radio_loading: bool,
    radio_advance: bool,
    radio_primed_for: Option<String>,
    radio_retry_at: Option<Instant>,
    radio_history: HashSet<String>,
    prefetch_enabled: bool,
    prefetch_track_id: Option<String>,
    pub marked: HashMap<String, Track>,
    undo_stack: Vec<UndoAction>,
    pub hits: Vec<HitRegion>,
    last_click: Option<(Instant, HitAction)>,
    pub mouse_enabled: bool,
    mpris: Option<Mpris>,
    mpris_signature: Option<String>,
    pub open_playlist: Option<usize>,
    pub playlist_track_selected: usize,
    pub plugins: PluginRegistry,
    pub plugin_selected: usize,
    pub active_metadata: Option<String>,
    pub active_streaming: Option<String>,
    pub active_lyrics: Option<String>,
    pub now_playing: Option<Track>,
    pub now_index: Option<usize>,
    pub audio: Option<AudioEngine>,
    pub audio_error: Option<String>,
    pub volume: f64,
    last_volume: f64,
    pub speed: f64,
    /// True while the sleep timer is ramping the volume down.
    sleep_fading: bool,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub status: String,
    pub toast: Option<(String, Instant)>,
    pub overlay: Option<Overlay>,
    pending_playlist_tracks: Vec<Track>,
    pub resolving: bool,
    pending_track_id: Option<String>,
    current_stream: Option<StreamInfo>,
    stream_cache: HashMap<String, (StreamInfo, Instant)>,
    stream_source: usize,
    playback_retries: u8,
    playback_failures: u8,
    /// When the resolve worker vanished (panic, kill) the reply never arrives;
    /// watch the clock so playback can never stall in "resolving" forever.
    resolve_started: Option<Instant>,
    /// A position to seek to once the next stream finishes loading. Used when
    /// a session is restored so it picks up mid-song instead of at zero.
    pending_seek: Option<f64>,
    /// The track and position written to disk when the previous session quit.
    resume_at: Option<f64>,
    resume_track_id: Option<String>,
    /// Which recent query the search field is currently cycling through.
    history_cursor: Option<usize>,
    events_tx: Sender<AppEvent>,
    events_rx: Receiver<AppEvent>,
    pub lyrics_cache: HashMap<String, LyricsDoc>,
    pub artwork: Option<Artwork>,
    pub artwork_track_id: Option<String>,
    artwork_url: Option<String>,
    pub spinner: usize,
    scrobbler: Scrobbler,
    recent_queries: Vec<String>,
    dirty_since: Option<Instant>,
}

impl App {
    pub fn new(config: AppConfig) -> Result<Self> {
        Self::build(config, false)
    }

    /// Build the app without any UI-only side effects (no cava subprocess) and
    /// without stealing the player from the daemon that launched us.
    pub fn new_headless(config: AppConfig) -> Result<Self> {
        Self::build(config, true)
    }

    fn build(config: AppConfig, headless: bool) -> Result<Self> {
        config::ensure_dirs()?;
        if !headless {
            crate::daemon::takeover();
        }
        let theme_name = theme::apply_configured(&config.theme);
        let cava_enabled = config.cava;
        let cava = (!headless && cava_enabled).then(|| Cava::spawn(config.cava_bars));
        let downloads = load_downloads();
        let mpris = config.mpris.then(Mpris::start);
        let (persisted, state_warning) = PersistedState::load();
        let mut plugins = PluginRegistry::load();
        let installed_defaults = plugins.ensure_defaults();

        let active_metadata = pick_active(
            &plugins,
            PluginKind::Metadata,
            persisted.active_metadata.as_deref(),
        );
        let active_streaming = pick_active(
            &plugins,
            PluginKind::Streaming,
            persisted.active_streaming.as_deref(),
        );
        let active_lyrics = pick_active(
            &plugins,
            PluginKind::Lyrics,
            persisted.active_lyrics.as_deref(),
        );

        let volume = persisted.volume.clamp(0.0, 130.0);
        let (audio, audio_error, attached, private_audio) =
            match AudioEngine::connect_or_spawn(volume, &config.audio_device) {
                Ok(engine) => {
                    let attached = engine.is_attached();
                    let private_audio = engine.is_private();
                    (Some(engine), None, attached, private_audio)
                }
                Err(err) => (None, Some(err.to_string()), false, false),
            };

        let repeat = match persisted.repeat.as_str() {
            "all" => RepeatMode::All,
            "one" => RepeatMode::One,
            _ => RepeatMode::Off,
        };

        // Remember where the previous session stopped so Enter can resume the
        // song mid-way instead of starting the whole track over.
        let resume_track_id = persisted.now_playing.as_ref().map(|track| track.id.clone());
        let resume_at = (persisted.position > 1.0).then_some(persisted.position);

        let now_playing = if attached {
            persisted.now_playing.clone()
        } else {
            None
        };
        let now_index = if attached { persisted.now_index } else { None };
        let restored_index = persisted
            .now_index
            .unwrap_or(0)
            .min(persisted.queue.len().saturating_sub(1));

        let library_paths = library::configured_paths(&config.library_paths);
        let library_tracks = library::load_cache();

        let (events_tx, events_rx) = mpsc::channel();
        let status = if attached {
            "attached to the background audio session".to_string()
        } else if !persisted.queue.is_empty() {
            "queue restored · press Enter to play".to_string()
        } else {
            "ready · press / to search".to_string()
        };

        let cfg_eq = config.eq.clone();
        let cfg_normalize = config.normalize;
        let cfg_radio = config.radio;
        let cfg_prefetch = config.prefetch;
        let cfg_mouse = config.mouse;
        let listenbrainz_token = config.listenbrainz_token.clone();
        let initial_view = match config.default_view.to_ascii_lowercase().as_str() {
            "queue" => View::Queue,
            "favorites" => View::Favorites,
            "playlists" => View::Playlists,
            "downloads" => View::Downloads,
            "plugins" => View::Plugins,
            _ => View::Search,
        };
        let mut app = Self {
            view: initial_view,
            config,
            theme_name,
            should_quit: false,
            detached_on_quit: false,
            search_query: String::new(),
            search_focused: false,
            search_results: SearchResults::default(),
            search_selected: 0,
            search_loading: false,
            search_error: None,
            search_dirty: false,
            search_generation: 0,
            last_search_edit: Instant::now(),
            search_cache: HashMap::new(),
            browse_stack: Vec::new(),
            browse_generation: 0,
            queue: persisted.queue,
            queue_selected: restored_index,
            favorites: persisted.favorites,
            favorites_selected: 0,
            history: persisted.history,
            history_selected: 0,
            library: library_tracks,
            library_selected: 0,
            library_scanning: false,
            library_paths,
            playlists: persisted.playlists,
            playlist_selected: 0,
            downloads,
            downloads_selected: 0,
            download_pending: HashSet::new(),
            cava,
            cava_enabled,
            sleep: SleepMode::Off,
            eq: if persisted.eq != "flat" {
                persisted.eq
            } else {
                cfg_eq.clone()
            },
            normalize: persisted.normalize || cfg_normalize,
            radio: persisted.radio || cfg_radio,
            radio_loading: false,
            radio_advance: false,
            radio_primed_for: None,
            radio_retry_at: None,
            radio_history: HashSet::new(),
            prefetch_enabled: cfg_prefetch,
            prefetch_track_id: None,
            marked: HashMap::new(),
            undo_stack: Vec::new(),
            hits: Vec::new(),
            last_click: None,
            mouse_enabled: cfg_mouse,
            mpris,
            mpris_signature: None,
            open_playlist: None,
            playlist_track_selected: 0,
            plugins,
            plugin_selected: 0,
            active_metadata,
            active_streaming,
            active_lyrics,
            now_playing,
            now_index,
            audio,
            audio_error,
            volume,
            last_volume: if volume > 0.0 { volume } else { 70.0 },
            speed: persisted.speed,
            sleep_fading: false,
            shuffle: persisted.shuffle,
            repeat,
            status,
            toast: None,
            overlay: None,
            pending_playlist_tracks: Vec::new(),
            resolving: false,
            pending_track_id: None,
            current_stream: None,
            stream_cache: HashMap::new(),
            stream_source: 0,
            playback_retries: 0,
            playback_failures: 0,
            resolve_started: None,
            pending_seek: None,
            resume_at,
            resume_track_id,
            history_cursor: None,
            events_tx,
            events_rx,
            lyrics_cache: HashMap::new(),
            artwork: None,
            artwork_track_id: None,
            artwork_url: None,
            spinner: 0,
            scrobbler: Scrobbler::new(listenbrainz_token),
            recent_queries: persisted.recent_queries,
            dirty_since: None,
        };

        if app.library.is_empty() && !app.library_paths.is_empty() {
            app.start_library_scan();
        }
        if !installed_defaults.is_empty() {
            app.flash(format!(
                "installed default plugins: {}",
                installed_defaults.join(", ")
            ));
        }
        if let Some(warning) = state_warning {
            app.flash(warning);
        }
        if let Some(err) = &app.audio_error {
            app.flash(err.clone());
        }
        if private_audio {
            app.flash("another session owns audio · this session uses its own player".to_string());
        }
        app.apply_audio_filters();
        if let Some(audio) = app.audio.as_ref() {
            let _ = audio.set_speed(app.speed);
        }
        if let Some(track) = app.now_playing.clone() {
            app.request_artwork(&track);
        }
        if let Some(err) = app.cava.as_ref().and_then(Cava::error) {
            app.flash(err.to_string());
        }
        app.clamp_selections();
        Ok(app)
    }

    pub fn tick(&mut self) {
        self.spinner = self.spinner.wrapping_add(1);

        while let Ok(event) = self.events_rx.try_recv() {
            self.handle_event(event);
        }

        self.poll_mpris();
        self.check_sleep_timer();
        self.maybe_prime_radio();
        if let Some(track) = self.now_playing.clone() {
            let position = self.audio_snapshot().position;
            self.scrobbler.tick(&track, position);
        }

        let ended = self
            .audio
            .as_ref()
            .map(AudioEngine::take_ended)
            .unwrap_or(false);
        let audio_error = self.audio.as_ref().and_then(AudioEngine::take_error);

        if self.audio_snapshot().position > 0.5 {
            self.playback_failures = 0;
        }

        if let Some(err) = audio_error {
            self.handle_playback_error(err);
        } else if ended && !self.resolving {
            self.on_track_ended();
        }

        // A resolve worker that panics (or gets killed) never posts its reply.
        // Without this the player would sit in "resolving…" forever, which is
        // exactly how the queue used to go silently dead.
        if self.resolving
            && self
                .resolve_started
                .is_some_and(|at| at.elapsed() > Duration::from_secs(90))
        {
            self.resolving = false;
            self.resolve_started = None;
            self.pending_track_id = None;
            if self.now_playing.is_some() {
                self.handle_playback_error("stream resolution timed out".to_string());
            } else {
                self.flash("stream resolution timed out");
            }
        }

        if self.search_dirty && self.last_search_edit.elapsed() >= Duration::from_millis(260) {
            self.start_search(false);
        }

        if self
            .dirty_since
            .is_some_and(|at| at.elapsed() >= Duration::from_secs(1))
        {
            self.persist();
            self.dirty_since = None;
        }

        if self
            .toast
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed() >= Duration::from_secs(4))
        {
            self.toast = None;
        }

        self.update_lyrics_follow();
        self.clamp_selections();
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.overlay.is_some() {
            self.handle_overlay_key(key);
            return;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') => {
                    self.should_quit = true;
                    return;
                }
                KeyCode::Char('p') => {
                    self.open_command_palette();
                    return;
                }
                KeyCode::Char('b') => {
                    self.detach_background();
                    return;
                }
                KeyCode::Char('f') => {
                    self.open_filter_current();
                    return;
                }
                KeyCode::Char('l') => {
                    self.open_link_import();
                    return;
                }
                KeyCode::Char('t') => {
                    self.toggle_radio();
                    return;
                }
                KeyCode::Char('g') => {
                    self.overlay = Some(Overlay::Stats);
                    return;
                }
                KeyCode::Char('v') => {
                    self.toggle_cava();
                    return;
                }
                KeyCode::Left => {
                    self.previous_track();
                    return;
                }
                KeyCode::Right => {
                    self.next_track(false);
                    return;
                }
                _ => {}
            }
        }

        if self.search_focused {
            self.handle_search_key(key);
            return;
        }

        match key.code {
            KeyCode::Tab => {
                self.next_view();
                return;
            }
            KeyCode::BackTab => {
                self.previous_view();
                return;
            }
            KeyCode::Char('1') => {
                if self.view == View::Search {
                    self.search_focused = true;
                } else {
                    self.switch_view(View::Search);
                }
                return;
            }
            KeyCode::Char('2') => {
                self.switch_view(View::Queue);
                return;
            }
            KeyCode::Char('3') => {
                self.switch_view(View::Favorites);
                return;
            }
            KeyCode::Char('4') => {
                self.switch_view(View::Playlists);
                return;
            }
            KeyCode::Char('5') => {
                self.switch_view(View::Downloads);
                return;
            }
            KeyCode::Char('6') => {
                self.switch_view(View::Plugins);
                return;
            }
            KeyCode::Char('7') => {
                self.switch_view(View::History);
                return;
            }
            KeyCode::Char('8') => {
                self.switch_view(View::Library);
                return;
            }
            _ => {}
        }

        match key.code {
            KeyCode::Char('?') => self.overlay = Some(Overlay::Help),
            KeyCode::Char(':') => self.open_command_palette(),
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('/') => self.open_filter_or_search(),
            KeyCode::Char(' ') => self.toggle_pause(),
            KeyCode::Char('n') if !key.modifiers.contains(KeyModifiers::ALT) => {
                self.next_track(false)
            }
            KeyCode::Char('b') => self.previous_track(),
            KeyCode::Char('+') | KeyCode::Char('=') => self.change_volume(self.config.volume_step),
            KeyCode::Char('-') | KeyCode::Char('_') => self.change_volume(-self.config.volume_step),
            KeyCode::Char('m') => self.toggle_mute(),
            KeyCode::Char('[') => self.change_speed(-0.1),
            KeyCode::Char(']') => self.change_speed(0.1),
            KeyCode::Char('\\') => self.reset_speed(),
            KeyCode::Char('z') => self.open_sleep_timer(),
            KeyCode::Char('E') => self.cycle_eq(),
            KeyCode::Char('v') => self.toggle_mark(),
            KeyCode::Char('V') => self.mark_all_current(),
            KeyCode::Char('S') => self.save_queue_as_playlist(),
            KeyCode::Char('d') if self.view != View::Playlists || self.open_playlist.is_some() => {
                self.download_selected()
            }
            KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::ALT) => {
                self.clear_upcoming()
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::ALT) => self.undo_last(),
            KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::ALT) => {
                self.export_playlists()
            }
            KeyCode::Char('i') if key.modifiers.contains(KeyModifiers::ALT) => {
                self.import_playlists()
            }
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::ALT) => {
                self.toggle_normalize()
            }
            KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::ALT) => self.cycle_theme(),
            KeyCode::Left if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.seek(-self.config.long_seek_step)
            }
            KeyCode::Right if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.seek(self.config.long_seek_step)
            }
            KeyCode::Left if key.modifiers.contains(KeyModifiers::ALT) => self.history_back(),
            KeyCode::Left => self.seek(-self.config.seek_step),
            KeyCode::Right => self.seek(self.config.seek_step),
            KeyCode::Char('h') => self.history_back(),
            KeyCode::Char('l') => self.toggle_favorite(),
            KeyCode::Char('N') => self.play_selected_next(),
            KeyCode::Char('t')
                if self.view == View::Queue && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                self.jump_to_now_playing()
            }
            KeyCode::Char('p') if self.view == View::Browse => self.play_browse_page(),
            KeyCode::Char('i') if self.view != View::Plugins => self.activate_selected(),
            KeyCode::Char('s') => self.toggle_shuffle(),
            KeyCode::Char('r') => self.cycle_repeat(),
            KeyCode::Char('y') => self.open_lyrics(),
            KeyCode::Char('Y') => self.share_now_playing(),
            KeyCode::Char('F') => self.toggle_favorite(),
            KeyCode::Char('P') => self.begin_add_to_playlist(),
            KeyCode::Char('a') => self.queue_selected(),
            KeyCode::Char('A') => self.queue_all_current(),
            KeyCode::Char('x') => self.remove_current_context(),
            KeyCode::Char('C') => {
                if !self.queue.is_empty() {
                    self.overlay = Some(Overlay::Confirm {
                        message: "Clear the whole queue and stop playback?".to_string(),
                        action: ConfirmAction::ClearQueue,
                    });
                }
            }
            KeyCode::Char('J') if self.view == View::Queue => self.move_queue(1),
            KeyCode::Char('K') if self.view == View::Queue => self.move_queue(-1),
            KeyCode::Char('g') => self.goto_top(),
            KeyCode::Char('G') => self.goto_bottom(),
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::Home => self.goto_top(),
            KeyCode::End => self.goto_bottom(),
            KeyCode::Enter => self.activate_selected(),
            KeyCode::Esc => {
                if self.view == View::Browse {
                    self.browse_back();
                } else if self.open_playlist.is_some() {
                    self.open_playlist = None;
                }
            }
            KeyCode::Backspace if self.view == View::Browse => self.browse_back(),
            KeyCode::Char('c') if self.view == View::Playlists => {
                self.pending_playlist_tracks.clear();
                self.overlay = Some(Overlay::NewPlaylist {
                    name: String::new(),
                });
            }
            KeyCode::Char('d') if self.view == View::Playlists && self.open_playlist.is_none() => {
                if let Some(playlist) = self.playlists.get(self.playlist_selected) {
                    self.overlay = Some(Overlay::Confirm {
                        message: format!("Delete playlist “{}”?", playlist.name),
                        action: ConfirmAction::DeletePlaylist(self.playlist_selected),
                    });
                }
            }
            KeyCode::Char('p') if self.view == View::Playlists => {
                if let Some(index) = self.open_playlist {
                    self.play_playlist(index);
                } else if self.playlist_selected < self.playlists.len() {
                    let index = self.playlist_selected;
                    self.play_playlist(index);
                }
            }
            KeyCode::Char('i') if self.view == View::Plugins => self.install_selected_plugin(),
            KeyCode::Char('u') if self.view == View::Plugins => self.uninstall_selected_plugin(),
            KeyCode::Char('e') if self.view == View::Playlists => {
                let index = self.open_playlist.unwrap_or(self.playlist_selected);
                if let Some(playlist) = self.playlists.get(index) {
                    self.overlay = Some(Overlay::RenamePlaylist {
                        index,
                        name: playlist.name.clone(),
                    });
                }
            }
            KeyCode::Char('J') if self.view == View::Playlists && self.open_playlist.is_some() => {
                self.move_playlist_track(1)
            }
            KeyCode::Char('K') if self.view == View::Playlists && self.open_playlist.is_some() => {
                self.move_playlist_track(-1)
            }
            KeyCode::Char('R') => self.refresh_current(),
            _ => {}
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                if self.search_query.is_empty() {
                    self.search_focused = false;
                } else {
                    // First Esc clears the field, second leaves it.
                    self.search_query.clear();
                    self.on_search_edited();
                }
            }
            KeyCode::Tab => {
                self.search_focused = false;
                self.next_view();
            }
            KeyCode::BackTab => {
                self.search_focused = false;
                self.previous_view();
            }
            KeyCode::Enter => {
                if self.search_dirty || self.search_results.is_empty() {
                    self.start_search(true);
                } else {
                    self.search_focused = false;
                    self.play_selected_search();
                }
            }
            KeyCode::Up => {
                if self.history_cursor.is_some() || self.search_query.trim().is_empty() {
                    self.recall_recent(-1);
                } else {
                    self.move_selection(-1);
                }
            }
            KeyCode::Down => {
                if self.history_cursor.is_some() || self.search_query.trim().is_empty() {
                    self.recall_recent(1);
                } else {
                    self.move_selection(1);
                }
            }
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::Home => self.goto_top(),
            KeyCode::End => self.goto_bottom(),
            KeyCode::Backspace => {
                self.search_query.pop();
                self.on_search_edited();
            }
            KeyCode::Delete => {
                self.search_query.clear();
                self.on_search_edited();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search_query.clear();
                self.on_search_edited();
            }
            KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let trimmed = self.search_query.trim_end();
                let cut = trimmed
                    .rfind(char::is_whitespace)
                    .map(|i| i + 1)
                    .unwrap_or(0);
                self.search_query.truncate(cut);
                self.on_search_edited();
            }
            KeyCode::Char(ch)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                self.search_query.push(ch);
                self.on_search_edited();
            }
            _ => {}
        }
    }

    fn handle_overlay_key(&mut self, key: KeyEvent) {
        let Some(overlay) = self.overlay.take() else {
            return;
        };
        match overlay {
            Overlay::Help | Overlay::Stats => {
                // Any key dismisses help / stats.
            }
            Overlay::Confirm { message, action } => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    self.run_confirm(action);
                }
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('q') => {}
                _ => {
                    self.overlay = Some(Overlay::Confirm { message, action });
                }
            },
            Overlay::Sleep { mut selected } => match key.code {
                KeyCode::Esc => {}
                KeyCode::Up => {
                    selected = selected.saturating_sub(1);
                    self.overlay = Some(Overlay::Sleep { selected });
                }
                KeyCode::Down => {
                    selected = (selected + 1).min(SLEEP_OPTIONS.len() - 1);
                    self.overlay = Some(Overlay::Sleep { selected });
                }
                KeyCode::Char('j') => {
                    selected = (selected + 1).min(SLEEP_OPTIONS.len() - 1);
                    self.overlay = Some(Overlay::Sleep { selected });
                }
                KeyCode::Char('k') => {
                    selected = selected.saturating_sub(1);
                    self.overlay = Some(Overlay::Sleep { selected });
                }
                KeyCode::Enter => self.apply_sleep_choice(selected),
                _ => self.overlay = Some(Overlay::Sleep { selected }),
            },
            Overlay::ImportLink { mut input } => match key.code {
                KeyCode::Esc => {}
                KeyCode::Enter => {
                    let url = input.trim().to_string();
                    if url.is_empty() {
                        self.flash("paste a link first");
                    } else {
                        self.start_link_import(url);
                    }
                }
                KeyCode::Backspace => {
                    input.pop();
                    self.overlay = Some(Overlay::ImportLink { input });
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    input.clear();
                    self.overlay = Some(Overlay::ImportLink { input });
                }
                KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    match clipboard_text() {
                        Some(text) => {
                            input.clear();
                            input.push_str(text.trim());
                            self.overlay = Some(Overlay::ImportLink { input });
                        }
                        None => self.flash("clipboard is empty or unavailable"),
                    }
                }
                KeyCode::Char(ch)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    input.push(ch);
                    self.overlay = Some(Overlay::ImportLink { input });
                }
                _ => self.overlay = Some(Overlay::ImportLink { input }),
            },
            Overlay::CommandPalette {
                mut query,
                mut selected,
            } => match key.code {
                KeyCode::Esc => {}
                KeyCode::Up => {
                    selected = selected.saturating_sub(1);
                    self.overlay = Some(Overlay::CommandPalette { query, selected });
                }
                KeyCode::Down => {
                    let count = palette_ranked(&query).len();
                    if selected + 1 < count {
                        selected += 1;
                    }
                    self.overlay = Some(Overlay::CommandPalette { query, selected });
                }
                KeyCode::PageUp => {
                    selected = selected.saturating_sub(8);
                    self.overlay = Some(Overlay::CommandPalette { query, selected });
                }
                KeyCode::PageDown => {
                    let count = palette_ranked(&query).len();
                    selected = (selected + 8).min(count.saturating_sub(1));
                    self.overlay = Some(Overlay::CommandPalette { query, selected });
                }
                KeyCode::Enter => {
                    let ranked = palette_ranked(&query);
                    if let Some(index) = ranked.get(selected) {
                        let action = PALETTE_ITEMS[*index].action;
                        self.execute_palette_action(action);
                    } else {
                        self.overlay = Some(Overlay::CommandPalette { query, selected });
                    }
                }
                KeyCode::Backspace => {
                    query.pop();
                    selected = 0;
                    self.overlay = Some(Overlay::CommandPalette { query, selected });
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    query.clear();
                    selected = 0;
                    self.overlay = Some(Overlay::CommandPalette { query, selected });
                }
                KeyCode::Char(ch)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    query.push(ch);
                    selected = 0;
                    self.overlay = Some(Overlay::CommandPalette { query, selected });
                }
                _ => {
                    self.overlay = Some(Overlay::CommandPalette { query, selected });
                }
            },
            Overlay::Filter {
                kind,
                mut query,
                mut selected,
            } => match key.code {
                KeyCode::Esc => {}
                KeyCode::Up => {
                    selected = selected.saturating_sub(1);
                    self.overlay = Some(Overlay::Filter {
                        kind,
                        query,
                        selected,
                    });
                }
                KeyCode::Down => {
                    let count = self.filter_ranked(kind, &query).len();
                    if selected + 1 < count {
                        selected += 1;
                    }
                    self.overlay = Some(Overlay::Filter {
                        kind,
                        query,
                        selected,
                    });
                }
                KeyCode::PageUp => {
                    selected = selected.saturating_sub(8);
                    self.overlay = Some(Overlay::Filter {
                        kind,
                        query,
                        selected,
                    });
                }
                KeyCode::PageDown => {
                    let count = self.filter_ranked(kind, &query).len();
                    selected = (selected + 8).min(count.saturating_sub(1));
                    self.overlay = Some(Overlay::Filter {
                        kind,
                        query,
                        selected,
                    });
                }
                KeyCode::Enter => {
                    if let Some(track) = self.filter_track(kind, &query, selected) {
                        self.overlay = None;
                        self.play_track_now(track);
                    } else {
                        self.overlay = Some(Overlay::Filter {
                            kind,
                            query,
                            selected,
                        });
                    }
                }
                KeyCode::Backspace => {
                    query.pop();
                    selected = 0;
                    self.overlay = Some(Overlay::Filter {
                        kind,
                        query,
                        selected,
                    });
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    query.clear();
                    selected = 0;
                    self.overlay = Some(Overlay::Filter {
                        kind,
                        query,
                        selected,
                    });
                }
                KeyCode::Char(ch)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    query.push(ch);
                    selected = 0;
                    self.overlay = Some(Overlay::Filter {
                        kind,
                        query,
                        selected,
                    });
                }
                _ => {
                    self.overlay = Some(Overlay::Filter {
                        kind,
                        query,
                        selected,
                    });
                }
            },
            Overlay::PlaylistPicker { mut selected } => {
                let total = self.playlists.len() + 1;
                match key.code {
                    KeyCode::Esc => {
                        self.pending_playlist_tracks.clear();
                    }
                    KeyCode::Up => {
                        selected = selected.saturating_sub(1);
                        self.overlay = Some(Overlay::PlaylistPicker { selected });
                    }
                    KeyCode::Down => {
                        if selected + 1 < total {
                            selected += 1;
                        }
                        self.overlay = Some(Overlay::PlaylistPicker { selected });
                    }
                    KeyCode::Enter => {
                        if selected < self.playlists.len() {
                            let tracks = std::mem::take(&mut self.pending_playlist_tracks);
                            self.add_tracks_to_playlist(selected, tracks);
                        } else {
                            self.overlay = Some(Overlay::NewPlaylist {
                                name: String::new(),
                            });
                        }
                    }
                    _ => {
                        self.overlay = Some(Overlay::PlaylistPicker { selected });
                    }
                }
            }
            Overlay::NewPlaylist { mut name } => match key.code {
                KeyCode::Esc => {
                    self.pending_playlist_tracks.clear();
                }
                KeyCode::Enter => {
                    let tracks = std::mem::take(&mut self.pending_playlist_tracks);
                    self.create_playlist(&name, tracks);
                }
                KeyCode::Backspace => {
                    name.pop();
                    self.overlay = Some(Overlay::NewPlaylist { name });
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    name.clear();
                    self.overlay = Some(Overlay::NewPlaylist { name });
                }
                KeyCode::Char(ch)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    name.push(ch);
                    self.overlay = Some(Overlay::NewPlaylist { name });
                }
                _ => {
                    self.overlay = Some(Overlay::NewPlaylist { name });
                }
            },
            Overlay::RenamePlaylist { index, mut name } => match key.code {
                KeyCode::Esc => {}
                KeyCode::Enter => {
                    let trimmed = name.trim().to_string();
                    if trimmed.is_empty() {
                        self.overlay = Some(Overlay::RenamePlaylist { index, name });
                    } else if let Some(playlist) = self.playlists.get_mut(index) {
                        playlist.name = trimmed.clone();
                        self.flash(format!("renamed playlist to “{trimmed}”"));
                        self.mark_dirty();
                    }
                }
                KeyCode::Backspace => {
                    name.pop();
                    self.overlay = Some(Overlay::RenamePlaylist { index, name });
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    name.clear();
                    self.overlay = Some(Overlay::RenamePlaylist { index, name });
                }
                KeyCode::Char(ch)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    name.push(ch);
                    self.overlay = Some(Overlay::RenamePlaylist { index, name });
                }
                _ => {
                    self.overlay = Some(Overlay::RenamePlaylist { index, name });
                }
            },
            Overlay::Lyrics(mut lyrics) => match key.code {
                KeyCode::Esc | KeyCode::Char('y') | KeyCode::Char('q') => {}
                KeyCode::Char('j') | KeyCode::Down => {
                    lyrics.scroll = lyrics.scroll.saturating_add(1);
                    lyrics.follow = false;
                    self.overlay = Some(Overlay::Lyrics(lyrics));
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    lyrics.scroll = lyrics.scroll.saturating_sub(1);
                    lyrics.follow = false;
                    self.overlay = Some(Overlay::Lyrics(lyrics));
                }
                KeyCode::PageDown => {
                    lyrics.scroll = lyrics.scroll.saturating_add(10);
                    lyrics.follow = false;
                    self.overlay = Some(Overlay::Lyrics(lyrics));
                }
                KeyCode::PageUp => {
                    lyrics.scroll = lyrics.scroll.saturating_sub(10);
                    lyrics.follow = false;
                    self.overlay = Some(Overlay::Lyrics(lyrics));
                }
                KeyCode::Char('g') => {
                    lyrics.scroll = 0;
                    lyrics.follow = false;
                    self.overlay = Some(Overlay::Lyrics(lyrics));
                }
                KeyCode::Char('G') => {
                    lyrics.scroll = lyrics.lines.len().saturating_sub(1);
                    lyrics.follow = false;
                    self.overlay = Some(Overlay::Lyrics(lyrics));
                }
                KeyCode::Char('f') => {
                    lyrics.follow = !lyrics.follow;
                    self.overlay = Some(Overlay::Lyrics(lyrics));
                }
                KeyCode::Char(' ') => {
                    self.toggle_pause();
                    self.overlay = Some(Overlay::Lyrics(lyrics));
                }
                _ => {
                    self.overlay = Some(Overlay::Lyrics(lyrics));
                }
            },
        }
    }

    fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::SearchDone {
                generation,
                cache_key,
                result,
            } => {
                if generation != self.search_generation {
                    return;
                }
                self.search_loading = false;
                match result {
                    Ok(mut results) => {
                        normalize_results(&mut results);
                        self.search_cache
                            .insert(cache_key, (results.clone(), Instant::now()));
                        self.search_results = results;
                        self.search_selected = 0;
                        self.search_error = None;
                        self.status = if self.search_results.is_empty() {
                            format!("no results for “{}”", self.search_query)
                        } else {
                            format!(
                                "{} songs · {} artists · {} albums",
                                self.search_results.tracks.len(),
                                self.search_results.artists.len(),
                                self.search_results.albums.len()
                            )
                        };
                    }
                    Err(err) => {
                        self.search_error = Some(err.clone());
                        self.search_results = SearchResults::default();
                        self.search_selected = 0;
                        self.flash(format!("search failed: {err}"));
                    }
                }
            }
            AppEvent::BrowseDone { generation, result } => {
                if generation != self.browse_generation {
                    return;
                }
                let Some(page) = self.browse_stack.pop() else {
                    return;
                };
                let mut page = page;
                match result {
                    Ok(mut results) => {
                        normalize_results(&mut results);
                        if page.artwork.is_none() {
                            page.artwork = results
                                .artists
                                .first()
                                .and_then(|artist| artist.artwork.clone())
                                .or_else(|| {
                                    results
                                        .albums
                                        .first()
                                        .and_then(|album| album.artwork.clone())
                                });
                        }
                        page.albums = results.albums;
                        page.tracks = results.tracks;
                        page.loading = false;
                        page.error = None;
                        page.selected = 0;
                        self.status = format!(
                            "{} · {} albums · {} tracks",
                            page.title,
                            page.albums.len(),
                            page.tracks.len()
                        );
                    }
                    Err(err) => {
                        page.loading = false;
                        page.error = Some(err.clone());
                        self.flash(format!("browse failed: {err}"));
                    }
                }
                self.browse_stack.push(page);
            }
            AppEvent::StreamResolved { track_id, result } => {
                if self.pending_track_id.as_deref() != Some(track_id.as_str()) {
                    return;
                }
                self.resolving = false;
                self.pending_track_id = None;
                self.resolve_started = None;
                match result {
                    Ok(info) => {
                        if self
                            .now_playing
                            .as_ref()
                            .is_none_or(|track| track.id != track_id)
                        {
                            // Stale reply: the user moved on while this was
                            // resolving. Re-resolve whatever is now current
                            // so the player never ends up sitting silent.
                            if let Some(current) = self.now_playing.clone() {
                                self.resolve_track(current, self.stream_source);
                            }
                            return;
                        }
                        // A 30-second clip is not a playable track: fall through
                        // to the next streaming plugin instead of going silent
                        // halfway through the song.
                        if info.short_preview
                            && self
                                .try_alternate_stream(&track_id, "only a preview clip is available")
                        {
                            return;
                        }
                        if info.duration.is_some() {
                            if let Some(track) = self.now_playing.as_mut() {
                                if track.duration.is_none() {
                                    track.duration = info.duration;
                                    if let Some(index) = self.now_index {
                                        if let Some(queued) = self.queue.get_mut(index) {
                                            if queued.id == track.id {
                                                queued.duration = info.duration;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        self.stream_cache
                            .insert(track_id.clone(), (info.clone(), Instant::now()));
                        self.current_stream = Some(info.clone());
                        if let Some(audio) = self.audio.as_ref() {
                            let start = self.pending_seek.take().unwrap_or(0.0);
                            match audio.load_from(&info, start) {
                                Ok(()) => {
                                    let via = info
                                        .extractor
                                        .clone()
                                        .or_else(|| self.active_streaming.clone())
                                        .unwrap_or_else(|| "streaming plugin".to_string());
                                    self.status = format!("playing via {via}");
                                    self.mark_dirty();
                                    self.prefetch_next();
                                }
                                Err(err) => {
                                    let err = err.to_string();
                                    self.flash(format!("could not load stream: {err}"));
                                    // Walk the fallback chain instead of
                                    // leaving the player silent.
                                    self.handle_playback_error(err);
                                }
                            }
                        }
                    }
                    Err(err) => {
                        // A failure for a track the user already skipped past
                        // must not disturb what is playing now.
                        if self
                            .now_playing
                            .as_ref()
                            .is_some_and(|track| track.id != track_id)
                        {
                            return;
                        }
                        if self.try_alternate_stream(&track_id, "this provider found no stream") {
                            return;
                        }
                        self.flash(format!("stream resolution failed: {err}"));
                        self.handle_playback_error(err);
                    }
                }
            }
            AppEvent::PrefetchDone { track_id, result } => {
                self.handle_prefetch_done(track_id, result);
            }
            AppEvent::RadioDone { tracks } => {
                self.handle_radio_done(tracks);
            }
            AppEvent::LinkImportDone { result } => {
                self.handle_link_import_done(result);
            }
            AppEvent::DownloadDone {
                track,
                path,
                result,
            } => {
                self.download_pending.remove(&track.id);
                match result {
                    Ok(()) => {
                        self.downloads
                            .insert(track.id.clone(), (track.clone(), path));
                        self.flash(format!("downloaded “{}” for offline playback", track.title));
                        self.mark_dirty();
                    }
                    Err(err) => {
                        self.flash(format!("download failed: {err}"));
                    }
                }
            }
            AppEvent::ArtworkDone { track_id, result } => {
                if self.now_playing.as_ref().map(|t| t.id.as_str()) == Some(track_id.as_str()) {
                    match result {
                        Ok(art) => {
                            self.artwork = Some(art);
                            self.artwork_track_id = Some(track_id);
                        }
                        Err(_) => {
                            // Broken cover link is not worth a toast; the
                            // inspector simply keeps its text layout.
                            self.artwork = None;
                            self.artwork_track_id = None;
                        }
                    }
                }
            }
            AppEvent::ShareDone { result } => match result {
                Ok(line) => self.flash(format!("copied: {line}")),
                Err(err) => self.flash(format!("share failed: {err}")),
            },
            AppEvent::LibraryDone { tracks, error } => {
                self.library_scanning = false;
                if let Some(error) = error {
                    self.flash(format!("library scan failed: {error}"));
                } else {
                    let count = tracks.len();
                    self.library = tracks;
                    self.library_selected = 0;
                    let _ = library::save_cache(&self.library);
                    self.flash(format!("library: {count} local tracks"));
                    self.mark_dirty();
                }
            }
            AppEvent::LyricsDone { track_id, result } => match result {
                Ok(doc) => {
                    self.lyrics_cache.insert(track_id.clone(), doc.clone());
                    if let Some(Overlay::Lyrics(lyrics)) = self.overlay.as_mut() {
                        if lyrics.track.id == track_id {
                            lyrics.lines = parse_lyrics(&doc);
                            lyrics.doc = Some(doc);
                            lyrics.loading = false;
                            lyrics.error = None;
                            lyrics.scroll = 0;
                        }
                    }
                }
                Err(err) => {
                    if let Some(Overlay::Lyrics(lyrics)) = self.overlay.as_mut() {
                        if lyrics.track.id == track_id {
                            lyrics.loading = false;
                            lyrics.error = Some(err.clone());
                        }
                    }
                    self.flash(format!("lyrics failed: {err}"));
                }
            },
        }
    }

    fn poll_mpris(&mut self) {
        let commands: Vec<mpris::Command> = self
            .mpris
            .as_ref()
            .map(|mpris| mpris.commands.try_iter().collect())
            .unwrap_or_default();
        for command in commands {
            self.handle_mpris_command(command);
        }

        let Some(mpris) = self.mpris.as_ref() else {
            return;
        };
        let snapshot = self.audio_snapshot();
        let duration = snapshot.duration.max(
            self.now_playing
                .as_ref()
                .and_then(|track| track.duration)
                .map(|duration| duration as f64)
                .unwrap_or(0.0),
        );
        let update = mpris::Update {
            title: self
                .now_playing
                .as_ref()
                .map(|track| track.title.clone())
                .unwrap_or_default(),
            artist: self
                .now_playing
                .as_ref()
                .map(|track| track.artist.clone())
                .unwrap_or_default(),
            album: self
                .now_playing
                .as_ref()
                .map(|track| track.album.clone())
                .unwrap_or_default(),
            art_url: self
                .now_playing
                .as_ref()
                .and_then(|track| track.artwork.clone()),
            duration_secs: duration,
            position_secs: snapshot.position,
            paused: snapshot.paused,
            has_track: self.now_playing.is_some() && !snapshot.idle,
            volume: self.volume,
            can_next: !self.queue.is_empty(),
            can_previous: !self.queue.is_empty(),
            shuffle: self.shuffle,
            repeat: match self.repeat {
                RepeatMode::Off => "off",
                RepeatMode::All => "all",
                RepeatMode::One => "one",
            }
            .to_string(),
        };
        let signature = format!(
            "{}|{}|{}|{}|{}|{}|{}|{:.0}|{:.0}|{}|{}|{}",
            update.title,
            update.artist,
            update.album,
            update.paused,
            update.has_track,
            update.shuffle,
            update.repeat,
            update.volume,
            update.position_secs,
            update.duration_secs,
            update.can_next,
            update.can_previous
        );
        if self.mpris_signature.as_deref() != Some(signature.as_str()) {
            mpris.push(update);
            self.mpris_signature = Some(signature);
        }
    }

    fn handle_mpris_command(&mut self, command: mpris::Command) {
        match command {
            mpris::Command::Play => {
                let idle = self.audio_snapshot().idle;
                if idle {
                    self.resume_queue();
                } else if let Some(audio) = self.audio.as_ref() {
                    let _ = audio.set_pause(false);
                    self.status = "playing".to_string();
                }
            }
            mpris::Command::Pause => {
                if let Some(audio) = self.audio.as_ref() {
                    let _ = audio.set_pause(true);
                    self.status = "paused".to_string();
                }
            }
            mpris::Command::PlayPause => self.toggle_pause(),
            mpris::Command::Next => self.next_track(false),
            mpris::Command::Previous => self.previous_track(),
            mpris::Command::Stop => {
                if let Some(audio) = self.audio.as_ref() {
                    let _ = audio.stop();
                }
                self.status = "stopped".to_string();
            }
            mpris::Command::Seek(delta) => self.seek(delta),
            mpris::Command::SetPosition(position) => self.seek_absolute(position),
            mpris::Command::SetVolume(volume) => {
                self.volume = volume.clamp(0.0, 130.0);
                if self.volume > 0.0 {
                    self.last_volume = self.volume;
                }
                self.apply_volume();
            }
            mpris::Command::SetShuffle(shuffle) => {
                if self.shuffle != shuffle {
                    self.toggle_shuffle();
                }
            }
            mpris::Command::SetLoop(label) => {
                self.repeat = match label.as_str() {
                    "one" => RepeatMode::One,
                    "all" => RepeatMode::All,
                    _ => RepeatMode::Off,
                };
                self.status = self.repeat.label().to_string();
                self.mark_dirty();
            }
            mpris::Command::Quit => self.should_quit = true,
        }
    }

    fn check_sleep_timer(&mut self) {
        let SleepMode::At(deadline) = self.sleep else {
            return;
        };
        let remaining = deadline.saturating_duration_since(Instant::now());
        if !remaining.is_zero() {
            // Fade the last 15 seconds so the music drifts off instead of
            // getting guillotined mid-chorus.
            const FADE: f64 = 15.0;
            let secs = remaining.as_secs_f64();
            if secs <= FADE {
                let factor = (secs / FADE).clamp(0.0, 1.0);
                if let Some(audio) = self.audio.as_ref() {
                    let _ = audio.set_volume(self.volume * factor);
                }
                self.sleep_fading = true;
            }
            return;
        }
        self.sleep = SleepMode::Off;
        self.restore_sleep_volume();
        if let Some(audio) = self.audio.as_ref() {
            let _ = audio.set_pause(true);
        }
        self.flash("sleep timer — faded out and paused");
    }

    fn restore_sleep_volume(&mut self) {
        if !self.sleep_fading {
            return;
        }
        self.sleep_fading = false;
        if let Some(audio) = self.audio.as_ref() {
            let _ = audio.set_volume(self.volume);
        }
    }

    pub fn sleep_label(&self) -> Option<String> {
        match self.sleep {
            SleepMode::Off => None,
            SleepMode::EndOfTrack => Some("sleep: end of track".to_string()),
            SleepMode::At(deadline) => {
                let left = deadline.saturating_duration_since(Instant::now());
                let mins = left.as_secs() / 60;
                let secs = left.as_secs() % 60;
                Some(format!("sleep {mins}:{secs:02}"))
            }
        }
    }

    fn open_sleep_timer(&mut self) {
        let selected = match self.sleep {
            SleepMode::Off => 0,
            SleepMode::EndOfTrack => SLEEP_OPTIONS.len() - 1,
            SleepMode::At(deadline) => {
                let left = deadline.saturating_duration_since(Instant::now()).as_secs();
                SLEEP_OPTIONS
                    .iter()
                    .position(|(_, minutes)| {
                        *minutes != 0
                            && *minutes != u32::MAX
                            && (left / 60).abs_diff(*minutes as u64) < 2
                    })
                    .unwrap_or(0)
            }
        };
        self.overlay = Some(Overlay::Sleep { selected });
    }

    fn apply_sleep_choice(&mut self, selected: usize) {
        let Some((label, minutes)) = SLEEP_OPTIONS.get(selected).copied() else {
            return;
        };
        self.restore_sleep_volume();
        self.sleep = if minutes == 0 {
            SleepMode::Off
        } else if minutes == u32::MAX {
            SleepMode::EndOfTrack
        } else {
            SleepMode::At(Instant::now() + Duration::from_secs(u64::from(minutes) * 60))
        };
        self.flash(match self.sleep {
            SleepMode::Off => "sleep timer off".to_string(),
            SleepMode::EndOfTrack => "will pause after this track".to_string(),
            SleepMode::At(_) => format!("sleep timer set: {label}"),
        });
    }

    fn build_filters(&self) -> String {
        let mut filters = String::new();
        let preset = match self.eq.as_str() {
            "bass" => "equalizer=f=80:width_type=o:width=2:g=6,equalizer=f=220:width_type=o:width=2:g=3",
            "vocal" => "equalizer=f=250:width_type=o:width=2:g=-2,equalizer=f=3000:width_type=o:width=2:g=4",
            "night" => "dynaudnorm=f=200:g=7",
            "loud" => "loudnorm=I=-14:TP=-1.5:LRA=11",
            _ => "",
        };
        if !preset.is_empty() {
            filters.push_str(preset);
        }
        if self.normalize && self.eq != "loud" && self.eq != "night" {
            if !filters.is_empty() {
                filters.push(',');
            }
            filters.push_str("dynaudnorm=f=150:g=15");
        }
        filters
    }

    fn apply_audio_filters(&mut self) {
        let filters = self.build_filters();
        if let Some(audio) = self.audio.as_ref() {
            if let Err(err) = audio.set_filters(&filters) {
                self.flash(format!("audio filters failed: {err}"));
            }
        }
    }

    fn cycle_eq(&mut self) {
        const PRESETS: &[&str] = &["flat", "bass", "vocal", "night", "loud"];
        let next = PRESETS
            .iter()
            .position(|preset| *preset == self.eq)
            .map(|index| (index + 1) % PRESETS.len())
            .unwrap_or(0);
        self.eq = PRESETS[next].to_string();
        self.apply_audio_filters();
        self.flash(format!("equaliser: {}", self.eq));
        self.mark_dirty();
    }

    fn toggle_normalize(&mut self) {
        self.normalize = !self.normalize;
        self.apply_audio_filters();
        self.flash(if self.normalize {
            "loudness normalisation on"
        } else {
            "loudness normalisation off"
        });
        self.mark_dirty();
    }

    fn toggle_radio(&mut self) {
        self.radio = !self.radio;
        self.radio_primed_for = None;
        self.radio_retry_at = None;
        self.flash(if self.radio {
            "radio autoplay on — the queue keeps finding related tracks"
        } else {
            "radio autoplay off"
        });
        self.mark_dirty();
    }

    /// When the last queued song starts, fetch the next radio batch in the
    /// background. By the time the song ends the queue already has somewhere
    /// to go, so the radio transition is gapless instead of a long resolve.
    fn maybe_prime_radio(&mut self) {
        if !self.radio
            || self.radio_loading
            || self.repeat == RepeatMode::All
            || self.queue.is_empty()
        {
            return;
        }
        let Some(index) = self.now_index else {
            return;
        };
        if index + 1 < self.queue.len() {
            return;
        }
        let Some(track) = self.now_playing.as_ref() else {
            return;
        };
        if let Some(at) = self.radio_retry_at {
            if Instant::now() < at {
                return;
            }
        }
        if self.radio_primed_for.as_deref() == Some(track.id.as_str()) {
            return;
        }
        self.radio_primed_for = Some(track.id.clone());
        self.radio_advance = false;
        self.start_radio();
    }

    fn start_radio(&mut self) {
        if !self.radio || self.radio_loading {
            return;
        }
        let Some(track) = self.now_playing.clone() else {
            return;
        };
        let query = if track.artist.trim().is_empty() {
            track.title.clone()
        } else {
            track.artist.clone()
        };
        if query.trim().is_empty() {
            return;
        }
        let order = self.metadata_order();
        let Some(_primary) = order.first().cloned() else {
            self.status = "queue finished · no metadata plugin for radio".to_string();
            return;
        };
        // Try the artist first (a proper station), then the exact song, then
        // the bare title. Obscure artists and cover bands often return nothing
        // for an artist query; the title fallback still surfaces related
        // recordings so the queue never dies on a quiet search.
        let mut queries = Vec::new();
        if !track.artist.trim().is_empty() {
            queries.push(track.artist.clone());
        }
        if !track.title.trim().is_empty() {
            if !track.artist.trim().is_empty() {
                queries.push(format!("{} {}", track.title, track.artist));
            }
            queries.push(track.title.clone());
        }
        queries.dedup();
        self.radio_loading = true;
        self.status = format!("radio: finding more like {}…", query);
        let tx = self.events_tx.clone();
        thread::spawn(move || {
            let mut found = Vec::new();
            'queries: for query in queries {
                let args = vec!["search".to_string(), query];
                for plugin in &order {
                    if let Ok(raw) = run_plugin(plugin, &args, Duration::from_secs(25)) {
                        if let Ok(payload) = serde_json::from_str::<SearchPayload>(&raw) {
                            let results = payload.into_results();
                            if !results.tracks.is_empty() {
                                found = results.tracks;
                                break 'queries;
                            }
                        }
                    }
                }
            }
            let _ = tx.send(AppEvent::RadioDone { tracks: found });
        });
    }

    fn handle_radio_done(&mut self, tracks: Vec<Track>) {
        self.radio_loading = false;
        let advance = std::mem::take(&mut self.radio_advance);
        let current_id = self.now_playing.as_ref().map(|track| track.id.clone());
        let upcoming_start = self.now_index.map(|index| index + 1).unwrap_or(0);
        let queue_ids: HashSet<String> = self.queue.iter().map(|track| track.id.clone()).collect();
        // Do not let the radio restart the song that just ended or the few
        // songs before it; those are the most jarring repeats.
        let recent_ids: HashSet<String> = self
            .queue
            .iter()
            .take(upcoming_start)
            .rev()
            .take(4)
            .map(|track| track.id.clone())
            .collect();

        let mut added = 0usize;
        // Strict pass: nothing that is in the queue or recently played.
        for track in &tracks {
            if !track.is_valid()
                || queue_ids.contains(&track.id)
                || self.radio_history.contains(&track.id)
                || current_id.as_deref() == Some(track.id.as_str())
            {
                continue;
            }
            self.queue.push(track.clone());
            self.radio_history.insert(track.id.clone());
            added += 1;
            if added >= 30 {
                break;
            }
        }

        // Relaxed pass: the station only knows songs that are already in the
        // queue. Allow previously played songs to come back, but never the
        // current one or the immediate past few.
        if added == 0 {
            for track in &tracks {
                if !track.is_valid()
                    || current_id.as_deref() == Some(track.id.as_str())
                    || recent_ids.contains(&track.id)
                    || self
                        .queue
                        .iter()
                        .skip(upcoming_start)
                        .any(|item| item.id == track.id)
                {
                    continue;
                }
                self.queue.push(track.clone());
                added += 1;
                if added >= 15 {
                    break;
                }
            }
        }

        if added == 0 {
            self.radio_primed_for = None;
            self.radio_retry_at = Some(Instant::now() + Duration::from_secs(30));
            self.status = "queue finished · radio found nothing new".to_string();
            return;
        }
        self.radio_retry_at = None;
        self.mark_dirty();
        self.flash(format!("radio queued {added} related tracks"));
        if advance {
            self.next_track(true);
        }
    }

    fn prefetch_next(&mut self) {
        if !self.prefetch_enabled {
            return;
        }
        let Some(now) = self.now_index else {
            return;
        };
        let len = self.queue.len();
        if len < 2 {
            return;
        }
        let next = if now + 1 < len {
            now + 1
        } else if self.repeat == RepeatMode::All {
            0
        } else {
            return;
        };
        let Some(track) = self.queue.get(next).cloned() else {
            return;
        };
        if self.prefetch_track_id.as_deref() == Some(track.id.as_str()) {
            return;
        }
        if self
            .stream_cache
            .get(&track.id)
            .is_some_and(|(_, at)| at.elapsed() < Duration::from_secs(900))
        {
            return;
        }
        let order = self.streaming_order();
        let source = self.stream_source.min(order.len().saturating_sub(1));
        let Some(plugin) = order.get(source).cloned() else {
            return;
        };
        self.prefetch_track_id = Some(track.id.clone());
        let tx = self.events_tx.clone();
        thread::spawn(move || {
            let track_json = serde_json::to_string(&track).unwrap_or_else(|_| "{}".to_string());
            let args = vec!["resolve".to_string(), track_json];
            let result = run_plugin(&plugin, &args, Duration::from_secs(75))
                .and_then(|raw| {
                    let info: StreamInfo = serde_json::from_str(&raw)
                        .map_err(|err| anyhow::anyhow!("invalid plugin JSON: {err}"))?;
                    if info.url.trim().is_empty() {
                        return Err(anyhow::anyhow!("empty stream URL"));
                    }
                    Ok(info)
                })
                .map_err(|err| err.to_string());
            let _ = tx.send(AppEvent::PrefetchDone {
                track_id: track.id,
                result,
            });
        });
    }

    fn handle_prefetch_done(
        &mut self,
        track_id: String,
        result: std::result::Result<StreamInfo, String>,
    ) {
        if self.prefetch_track_id.as_deref() == Some(track_id.as_str()) {
            self.prefetch_track_id = None;
        }
        if let Ok(info) = result {
            if !info.url.trim().is_empty() && !info.short_preview {
                self.stream_cache.insert(track_id, (info, Instant::now()));
            }
        }
    }

    fn download_selected(&mut self) {
        let tracks = if !self.marked.is_empty() {
            let mut marked: Vec<Track> = self.marked.values().cloned().collect();
            marked.sort_by_key(|track| track.title.to_lowercase());
            marked
        } else {
            let tracks = self.context_tracks();
            if tracks.is_empty() {
                self.selected_track()
                    .map(|track| vec![track])
                    .unwrap_or_default()
            } else {
                tracks
            }
        };
        if tracks.is_empty() {
            self.flash("nothing selected to download");
            return;
        }
        for track in tracks {
            self.begin_download(track);
        }
    }

    fn begin_download(&mut self, track: Track) {
        if let Some((_, path)) = self.downloads.get(&track.id) {
            if path.exists() {
                self.flash(format!("already downloaded “{}”", track.title));
                return;
            }
        }
        if !self.download_pending.insert(track.id.clone()) {
            self.flash(format!("already downloading “{}”", track.title));
            return;
        }
        let dir = config::downloads_dir();
        if let Err(err) = std::fs::create_dir_all(&dir) {
            self.download_pending.remove(&track.id);
            self.flash(format!("could not create downloads folder: {err}"));
            return;
        }
        let stem = safe_filename(&format!("{} {}", track.artist, track.title));
        let stem = if stem.is_empty() {
            safe_filename(&track.id)
        } else {
            stem
        };
        let target = self
            .stream_cache
            .get(&track.id)
            .and_then(|(info, _)| info.webpage_url.clone())
            .filter(|url| !url.trim().is_empty())
            .unwrap_or_else(|| format!("ytsearch1:{} {}", track.title, track.artist));
        self.flash(format!("downloading “{}”…", truncate(&track.title, 34)));
        let tx = self.events_tx.clone();
        let dir_clone = dir.clone();
        thread::spawn(move || {
            let output = std::process::Command::new("yt-dlp")
                .arg("--no-playlist")
                .arg("--no-progress")
                .arg("--no-warnings")
                .arg("-f")
                .arg("bestaudio/best")
                .arg("-x")
                .arg("--audio-format")
                .arg("opus")
                .arg("-o")
                .arg(dir_clone.join(format!("{stem}.%(ext)s")))
                .arg(&target)
                .output();
            let result = match output {
                Ok(output) if output.status.success() => {
                    let mut found: Option<PathBuf> = None;
                    if let Ok(entries) = std::fs::read_dir(&dir_clone) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            let is_audio = path
                                .extension()
                                .and_then(|ext| ext.to_str())
                                .is_some_and(|ext| ext != "json" && ext != "part");
                            let matches = path
                                .file_stem()
                                .and_then(|name| name.to_str())
                                .is_some_and(|name| name == stem);
                            if is_audio && matches {
                                found = Some(path);
                                break;
                            }
                        }
                    }
                    match found {
                        Some(path) => {
                            let _ = std::fs::write(
                                dir_clone.join(format!("{stem}.json")),
                                serde_json::to_string(&track).unwrap_or_default(),
                            );
                            Ok(path)
                        }
                        None => Err("yt-dlp finished but no audio file was found".to_string()),
                    }
                }
                Ok(output) => {
                    let err = String::from_utf8_lossy(&output.stderr);
                    let tail = err.lines().last().unwrap_or("download failed");
                    Err(truncate(tail, 120))
                }
                Err(err) => Err(format!("could not run yt-dlp: {err}")),
            };
            match result {
                Ok(path) => {
                    let _ = tx.send(AppEvent::DownloadDone {
                        track,
                        path,
                        result: Ok(()),
                    });
                }
                Err(message) => {
                    let _ = tx.send(AppEvent::DownloadDone {
                        track,
                        path: PathBuf::new(),
                        result: Err(message),
                    });
                }
            }
        });
    }

    pub fn download_tracks(&self) -> Vec<Track> {
        let mut tracks: Vec<Track> = self
            .downloads
            .values()
            .map(|(track, _)| track.clone())
            .collect();
        tracks.sort_by(|a, b| {
            a.artist
                .to_lowercase()
                .cmp(&b.artist.to_lowercase())
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });
        tracks
    }

    fn download_path(&self, track_id: &str) -> Option<&PathBuf> {
        self.downloads.get(track_id).map(|(_, path)| path)
    }

    fn remove_download_selected(&mut self) {
        let tracks = self.download_tracks();
        let Some(track) = tracks.get(self.downloads_selected).cloned() else {
            return;
        };
        if let Some((_, path)) = self.downloads.remove(&track.id) {
            let _ = std::fs::remove_file(&path);
            if let Some(parent) = path.parent() {
                let stem = path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default();
                let _ = std::fs::remove_file(parent.join(format!("{stem}.json")));
            }
            self.flash(format!("deleted offline copy of “{}”", track.title));
            self.downloads_selected = self
                .downloads_selected
                .min(self.download_tracks().len().saturating_sub(1));
        }
    }

    fn toggle_mark(&mut self) {
        let Some(track) = self.selected_track() else {
            self.flash("nothing selected");
            return;
        };
        if self.marked.remove(&track.id).is_some() {
            self.flash(format!("unmarked “{}”", track.title));
        } else {
            self.marked.insert(track.id.clone(), track.clone());
            self.flash(format!(
                "marked “{}” · {} selected",
                track.title,
                self.marked.len()
            ));
        }
    }

    fn mark_all_current(&mut self) {
        let tracks = self.current_view_tracks();
        if tracks.is_empty() {
            self.flash("nothing to mark here");
            return;
        }
        for track in tracks {
            self.marked.insert(track.id.clone(), track);
        }
        self.flash(format!("marked {} tracks", self.marked.len()));
    }

    fn push_undo(&mut self, action: UndoAction) {
        self.undo_stack.push(action);
        if self.undo_stack.len() > 30 {
            self.undo_stack.remove(0);
        }
    }

    fn undo_last(&mut self) {
        let Some(action) = self.undo_stack.pop() else {
            self.flash("nothing to undo");
            return;
        };
        match action {
            UndoAction::Queue { track, index } => {
                let at = index.min(self.queue.len());
                self.queue.insert(at, track.clone());
                self.queue_selected = at;
                self.flash(format!("restored “{}” to the queue", track.title));
                if self.now_index.is_some_and(|now| at <= now) {
                    self.now_index = self.now_index.map(|now| now + 1);
                }
            }
            UndoAction::Favorites { track, index } => {
                let at = index.min(self.favorites.len());
                self.favorites.insert(at, track.clone());
                self.favorites_selected = at;
                self.flash(format!("restored “{}” to favorites", track.title));
            }
            UndoAction::Playlist {
                playlist,
                track,
                index,
            } => {
                if let Some(playlist) = self.playlists.get_mut(playlist) {
                    let at = index.min(playlist.tracks.len());
                    playlist.tracks.insert(at, track.clone());
                    self.playlist_track_selected = at;
                    self.flash(format!("restored “{}” to the playlist", track.title));
                }
            }
        }
        self.mark_dirty();
    }

    fn clear_upcoming(&mut self) {
        let keep = self
            .now_index
            .map(|index| (index + 1).min(self.queue.len()))
            .unwrap_or(0);
        if keep >= self.queue.len() {
            self.flash("nothing upcoming");
            return;
        }
        self.queue.truncate(keep);
        self.flash(format!(
            "cleared upcoming tracks · {} queued",
            self.queue.len()
        ));
        self.mark_dirty();
    }

    fn save_queue_as_playlist(&mut self) {
        if self.queue.is_empty() {
            self.flash("queue is empty");
            return;
        }
        self.pending_playlist_tracks = self.queue.clone();
        self.overlay = Some(Overlay::NewPlaylist {
            name: "Saved queue".to_string(),
        });
    }

    fn open_link_import(&mut self) {
        let input = clipboard_text()
            .filter(|text| text.starts_with("http://") || text.starts_with("https://"))
            .unwrap_or_default();
        self.overlay = Some(Overlay::ImportLink { input });
    }

    fn start_link_import(&mut self, url: String) {
        let url = url.trim().to_string();
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            self.flash("paste a full http(s) link");
            self.overlay = Some(Overlay::ImportLink { input: url });
            return;
        }
        self.overlay = None;
        self.status = "importing link — reading the playlist…".to_string();
        self.flash("importing link — reading the playlist…");
        let tx = self.events_tx.clone();
        thread::spawn(move || {
            let result = import_link(&url).map_err(|err| err.to_string());
            let _ = tx.send(AppEvent::LinkImportDone { result });
        });
    }

    fn handle_link_import_done(&mut self, result: std::result::Result<LinkImport, String>) {
        match result {
            Ok(import) => {
                let base = if import.title.trim().is_empty() {
                    "Imported link".to_string()
                } else {
                    import.title.trim().to_string()
                };
                let name = unique_playlist_name(&base, &self.playlists);
                let count = import.tracks.len();
                self.playlists.push(Playlist {
                    id: format!("playlist-{}", unix_nanos()),
                    name: name.clone(),
                    tracks: import.tracks.clone(),
                });
                self.playlist_selected = self.playlists.len().saturating_sub(1);

                let before = self.queue.len();
                for track in &import.tracks {
                    if !self.queue.iter().any(|item| item.id == track.id) {
                        self.queue.push(track.clone());
                    }
                }
                let added = self.queue.len() - before;
                self.mark_dirty();

                if count == 1 || self.now_playing.is_none() {
                    self.play_with_context(import.tracks[0].clone(), import.tracks.clone());
                    self.flash(format!(
                        "imported {count} track{} from “{name}” · playing",
                        if count == 1 { "" } else { "s" }
                    ));
                } else {
                    self.flash(format!(
                        "imported {count} tracks from “{name}” · {added} queued · saved as a playlist"
                    ));
                }
            }
            Err(err) => self.flash(format!("link import failed: {err}")),
        }
    }

    fn export_playlists(&mut self) {
        let path = config::config_dir().join("mfe-player.m3u");
        let mut out = String::from("#EXTM3U\n");
        for playlist in &self.playlists {
            out.push_str(&format!("#PLAYLIST:{}\n", playlist.name));
            for track in &playlist.tracks {
                out.push_str(&format!(
                    "#EXTINF:{},{}{}\n",
                    track.duration.unwrap_or(0),
                    if track.artist.is_empty() {
                        String::new()
                    } else {
                        format!("{} - ", track.artist)
                    },
                    track.title
                ));
                out.push('\n');
                if let Ok(json) = serde_json::to_string(track) {
                    out.push_str(&format!("#MFE:{json}\n"));
                }
            }
        }
        match std::fs::write(&path, out) {
            Ok(()) => self.flash(format!(
                "exported {} playlists to {}",
                self.playlists.len(),
                path.display()
            )),
            Err(err) => self.flash(format!("export failed: {err}")),
        }
    }

    fn import_playlists(&mut self) {
        let path = config::config_dir().join("mfe-player.m3u");
        let Ok(raw) = std::fs::read_to_string(&path) else {
            self.flash(format!("no playlist file at {}", path.display()));
            return;
        };
        let mut current: Option<Playlist> = None;
        let mut imported = 0usize;
        let mut pending_title: Option<String> = None;
        for line in raw.lines() {
            let line = line.trim();
            if let Some(name) = line.strip_prefix("#PLAYLIST:") {
                if let Some(playlist) = current.take() {
                    if !playlist.tracks.is_empty() {
                        self.playlists.push(playlist);
                        imported += 1;
                    }
                }
                current = Some(Playlist {
                    id: format!("playlist-{}", unix_nanos()),
                    name: name.to_string(),
                    tracks: Vec::new(),
                });
            } else if let Some(json) = line
                .strip_prefix("#MFE:")
                .or_else(|| line.strip_prefix("#NUCLEAR:"))
            {
                if let Ok(track) = serde_json::from_str::<Track>(json) {
                    if let Some(playlist) = current.as_mut() {
                        // The readable title line before #MFE created a
                        // placeholder. Upgrade it in place instead of adding a
                        // second copy of the same song.
                        let replace_placeholder = playlist.tracks.last().is_some_and(|last| {
                            last.id.starts_with("imported:")
                                && (last.title == track.title
                                    || last.title == format!("{} - {}", track.artist, track.title)
                                    || last.title.trim_end_matches(['-', ' ']).trim()
                                        == track.artist.trim())
                        });
                        if replace_placeholder {
                            if let Some(last) = playlist.tracks.last_mut() {
                                *last = track;
                            }
                        } else {
                            playlist.tracks.push(track);
                        }
                    }
                }
            } else if let Some(meta) = line.strip_prefix("#EXTINF:") {
                pending_title = meta.split_once(',').map(|(_, title)| title.to_string());
            } else if !line.is_empty() && !line.starts_with('#') {
                if let Some(playlist) = current.as_mut() {
                    if playlist.tracks.iter().all(|track| track.title != line) {
                        playlist.tracks.push(Track {
                            id: format!("imported:{}", line),
                            title: pending_title.take().unwrap_or_else(|| line.to_string()),
                            ..Default::default()
                        });
                    }
                }
            }
        }
        if let Some(playlist) = current.take() {
            if !playlist.tracks.is_empty() {
                self.playlists.push(playlist);
                imported += 1;
            }
        }
        if imported == 0 {
            self.flash("nothing importable in mfe-player.m3u");
        } else {
            self.playlist_selected = self.playlists.len().saturating_sub(1);
            self.flash(format!("imported {imported} playlists"));
            self.mark_dirty();
        }
    }

    fn cycle_theme(&mut self) {
        let names = theme::THEME_NAMES;
        let index = names
            .iter()
            .position(|name| *name == self.theme_name)
            .map(|index| (index + 1) % names.len())
            .unwrap_or(0);
        let name = names[index];
        theme::set_theme(name);
        self.theme_name = name.to_string();
        config::set_theme_in_file(name);
        self.flash(format!("theme: {name}"));
    }

    fn toggle_cava(&mut self) {
        if self.cava.is_some() {
            self.cava = None;
            self.cava_enabled = false;
            self.flash("spectrum visualiser off");
        } else {
            self.cava = Some(Cava::spawn(self.config.cava_bars));
            self.cava_enabled = true;
            match self.cava.as_ref().and_then(Cava::error) {
                Some(err) => self.flash(err.to_string()),
                None => self.flash("spectrum visualiser on"),
            }
        }
    }

    fn seek_absolute(&mut self, seconds: f64) {
        if let Some(audio) = self.audio.as_ref() {
            match audio.seek_absolute(seconds) {
                Ok(()) => self.status = format!("seek to {}", format_clock(seconds)),
                Err(err) => self.flash(format!("seek failed: {err}")),
            }
        }
    }

    /// Register clickable rows produced by the renderer.
    pub fn record_hits(&mut self, area: Rect, offset: usize, rows: Vec<(usize, HitAction)>) {
        for (line, action) in rows {
            if line < offset || line >= offset + area.height as usize {
                continue;
            }
            let y = area.y + (line - offset) as u16;
            if y >= area.y.saturating_add(area.height) {
                continue;
            }
            self.hits.push(HitRegion {
                area: Rect {
                    x: area.x,
                    y,
                    width: area.width,
                    height: 1,
                },
                action,
            });
        }
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        if !self.mouse_enabled {
            return;
        }
        match mouse.kind {
            MouseEventKind::ScrollDown => self.move_selection(1),
            MouseEventKind::ScrollUp => self.move_selection(-1),
            MouseEventKind::Down(MouseButton::Left) => {
                let Some(hit) = self
                    .hits
                    .iter()
                    .rev()
                    .find(|hit| rect_contains(hit.area, mouse.column, mouse.row))
                    .copied()
                else {
                    return;
                };
                match hit.action {
                    HitAction::SwitchView(view) => self.switch_view(view),
                    HitAction::SeekBar => {
                        let duration = self.audio_snapshot().duration.max(
                            self.now_playing
                                .as_ref()
                                .and_then(|track| track.duration)
                                .map(|duration| duration as f64)
                                .unwrap_or(0.0),
                        );
                        if duration <= 0.0 {
                            return;
                        }
                        let ratio = f64::from(mouse.column.saturating_sub(hit.area.x))
                            / f64::from(hit.area.width.max(1));
                        self.seek_absolute(ratio.clamp(0.0, 1.0) * duration);
                    }
                    HitAction::Select(view, index) => {
                        let double = self.last_click.as_ref().is_some_and(|(at, action)| {
                            *action == hit.action && at.elapsed() < Duration::from_millis(450)
                        });
                        self.select_hit(view, index);
                        if double {
                            self.activate_selected();
                        }
                        self.last_click = Some((Instant::now(), hit.action));
                    }
                }
            }
            _ => {}
        }
    }

    fn select_hit(&mut self, view: View, index: usize) {
        match view {
            View::Search => self.search_selected = index,
            View::Queue => self.queue_selected = index,
            View::Favorites => self.favorites_selected = index,
            View::Playlists => {
                if self.open_playlist.is_some() {
                    self.playlist_track_selected = index;
                } else {
                    self.playlist_selected = index;
                }
            }
            View::Downloads => self.downloads_selected = index,
            View::Plugins => self.plugin_selected = index,
            View::History => self.history_selected = index,
            View::Library => self.library_selected = index,
            View::Browse => {
                if let Some(page) = self.browse_stack.last_mut() {
                    page.selected = index;
                }
            }
        }
    }

    fn on_search_edited(&mut self) {
        self.history_cursor = None;
        self.search_generation = self.search_generation.wrapping_add(1);
        self.search_loading = false;
        self.search_dirty = true;
        self.last_search_edit = Instant::now();
        self.search_error = None;
        if self.search_query.trim().is_empty() {
            self.search_dirty = false;
            self.search_loading = false;
            self.search_results = SearchResults::default();
            self.search_selected = 0;
        }
    }

    fn start_search(&mut self, force: bool) {
        let query = self.search_query.trim().to_string();
        if query.is_empty() {
            self.search_dirty = false;
            return;
        }
        if !force && !self.search_dirty {
            return;
        }

        if let Some((cached, at)) = self.search_cache.get(&query.to_lowercase()) {
            if at.elapsed() < Duration::from_secs(300) {
                self.search_results = cached.clone();
                self.search_selected = 0;
                self.search_loading = false;
                self.search_dirty = false;
                self.search_error = None;
                self.status = format!(
                    "{} songs · {} artists · {} albums",
                    self.search_results.tracks.len(),
                    self.search_results.artists.len(),
                    self.search_results.albums.len()
                );
                return;
            }
        }

        let metadata_order = self.metadata_order();
        let Some(primary) = metadata_order.first() else {
            self.flash("no metadata plugin installed — open the plugin store (5)");
            self.search_loading = false;
            self.search_dirty = false;
            return;
        };

        self.search_generation = self.search_generation.wrapping_add(1);
        let generation = self.search_generation;
        self.search_loading = true;
        self.search_dirty = false;
        self.search_error = None;
        self.status = format!("searching {} via {}…", query, primary.manifest.name);
        self.remember_query(&query);

        let cache_key = query.to_lowercase();
        let tx = self.events_tx.clone();
        thread::spawn(move || {
            let args = vec!["search".to_string(), query];
            let mut errors = Vec::new();
            // If the active metadata plugin is down, walk the other installed
            // ones so search keeps working instead of dying on one provider.
            for plugin in metadata_order {
                let name = plugin.manifest.name.clone();
                match run_plugin(&plugin, &args, Duration::from_secs(20)).and_then(|raw| {
                    let payload: SearchPayload = serde_json::from_str(&raw)
                        .map_err(|err| anyhow::anyhow!("invalid plugin JSON: {err}"))?;
                    Ok(payload.into_results())
                }) {
                    Ok(results) => {
                        let _ = tx.send(AppEvent::SearchDone {
                            generation,
                            cache_key,
                            result: Ok(results),
                        });
                        return;
                    }
                    Err(err) => errors.push(format!("{name}: {err}")),
                }
            }
            let _ = tx.send(AppEvent::SearchDone {
                generation,
                cache_key,
                result: Err(errors.join(" · ")),
            });
        });
    }

    fn remember_query(&mut self, query: &str) {
        self.recent_queries.retain(|item| item != query);
        self.recent_queries.insert(0, query.to_string());
        self.recent_queries.truncate(12);
        self.mark_dirty();
    }

    /// Up/Down in an empty search field cycles through past queries, like a
    /// shell history. Down past the oldest entry clears the field again.
    fn recall_recent(&mut self, delta: isize) {
        if self.recent_queries.is_empty() {
            self.flash("no recent searches yet");
            return;
        }
        let len = self.recent_queries.len();
        let next = match self.history_cursor {
            Some(index) if delta < 0 => index.checked_sub(1),
            Some(index) => {
                let target = index as isize + delta;
                if target >= len as isize {
                    None
                } else {
                    Some(target as usize)
                }
            }
            None => {
                if delta < 0 {
                    Some(0)
                } else {
                    return;
                }
            }
        };
        self.search_query = next
            .and_then(|index| self.recent_queries.get(index).cloned())
            .unwrap_or_default();
        self.on_search_edited();
        self.history_cursor = next;
    }

    fn play_selected_search(&mut self) {
        match self.search_selected_row() {
            Some(SearchRow::Artist(index)) => {
                if let Some(artist) = self.search_results.artists.get(index).cloned() {
                    self.open_artist(artist);
                }
            }
            Some(SearchRow::Album(index)) => {
                if let Some(album) = self.search_results.albums.get(index).cloned() {
                    self.open_album(album);
                }
            }
            Some(SearchRow::Track(index)) => {
                if let Some(track) = self.search_results.tracks.get(index).cloned() {
                    // Keep autoplay rolling through the rest of the results.
                    let context = self.search_results.tracks.clone();
                    self.play_with_context(track, context);
                }
            }
            // Nothing selectable (empty results, header row): the startup
            // banner promises "press Enter to play" for a restored queue, so
            // honour it instead of doing nothing.
            _ => {
                if !self.resume_queue()
                    && self.search_query.trim().is_empty()
                    && self.queue.is_empty()
                {
                    self.flash("nothing to play — search for something first");
                }
            }
        }
    }

    /// Start (or restart) playback from where the restored queue left off.
    /// Returns true when something was queued to play.
    fn resume_queue(&mut self) -> bool {
        if self.queue.is_empty() || self.audio.is_none() {
            return false;
        }
        let snapshot = self.audio_snapshot();
        if !snapshot.idle && snapshot.position > 0.5 {
            // Something is already rolling: Enter un-pauses at most, it must
            // never restart the track that is currently mid-play.
            if snapshot.paused {
                if let Some(audio) = self.audio.as_ref() {
                    let _ = audio.set_pause(false);
                }
                self.flash("resumed");
            }
            return true;
        }
        // Prefer the track that was playing when the previous session quit,
        // then the persisted cursor, then the visible selection.
        let index = self
            .resume_track_id
            .as_deref()
            .and_then(|id| self.queue.iter().position(|track| track.id == id))
            .or(self.now_index)
            .unwrap_or(self.queue_selected)
            .min(self.queue.len() - 1);
        let track_id = self.queue[index].id.clone();
        let track_duration = self.queue[index].duration.map(|duration| duration as f64);
        let start_at = self
            .resume_at
            .take()
            .filter(|_| self.resume_track_id.as_deref() == Some(track_id.as_str()))
            .filter(|position| *position > 1.0)
            .filter(|position| track_duration.is_none_or(|duration| *position < duration - 5.0));
        self.play_queue_index_at(index, start_at);
        self.resume_track_id = None;
        self.flash(match start_at {
            Some(position) => format!("resuming at {}", format_clock(position)),
            None => "resuming queue".to_string(),
        });
        true
    }

    /// Spotify-style "play from here": start `selected` and line up the rest
    /// of its context (album, playlist, search results, favorites) behind it,
    /// so when the current song finishes the next one starts on its own.
    /// Whatever sat before the currently playing track in the queue is kept
    /// as history — only the future part of the queue is replaced.
    fn play_with_context(&mut self, selected: Track, context: Vec<Track>) {
        if self.audio.is_none() {
            let detail = self
                .audio_error
                .clone()
                .unwrap_or_else(|| "audio engine unavailable".to_string());
            self.flash(detail);
            return;
        }

        let keep = self
            .now_index
            .map(|index| (index + 1).min(self.queue.len()))
            .unwrap_or(0);
        let mut new_queue: Vec<Track> = self.queue.drain(..keep).collect();
        self.queue.clear();
        if context.is_empty() {
            new_queue.push(selected.clone());
        }
        for track in context {
            if !new_queue.iter().any(|item| item.id == track.id) {
                new_queue.push(track);
            }
        }
        self.queue = new_queue;

        let index = self
            .queue
            .iter()
            .position(|item| item.id == selected.id)
            .unwrap_or(self.queue.len().saturating_sub(1));
        if index >= self.queue.len() {
            self.flash("nothing to play here");
            return;
        }
        self.queue_selected = index;
        self.now_index = Some(index);
        self.now_playing = Some(self.queue[index].clone());
        self.pending_seek = None;
        self.resume_at = None;
        self.resume_track_id = None;
        self.mark_dirty();
        let track = self.queue[index].clone();
        self.start_stream(track);
    }

    fn play_track_now(&mut self, track: Track) {
        if self.audio.is_none() {
            let detail = self
                .audio_error
                .clone()
                .unwrap_or_else(|| "audio engine unavailable".to_string());
            self.flash(detail);
            return;
        }

        let existing = self.queue.iter().position(|item| item.id == track.id);
        let index = if let Some(index) = existing {
            index
        } else {
            let insert_at = self
                .now_index
                .map(|index| (index + 1).min(self.queue.len()))
                .unwrap_or(0);
            self.queue.insert(insert_at, track.clone());
            insert_at
        };
        self.queue_selected = index;
        self.now_index = Some(index);
        self.now_playing = Some(track.clone());
        self.pending_seek = None;
        self.resume_at = None;
        self.resume_track_id = None;
        self.mark_dirty();
        self.start_stream(track);
    }

    fn play_queue_index(&mut self, index: usize) {
        self.play_queue_index_at(index, None);
    }

    fn play_queue_index_at(&mut self, index: usize, start_at: Option<f64>) {
        let Some(track) = self.queue.get(index).cloned() else {
            return;
        };
        self.now_index = Some(index);
        self.queue_selected = index;
        self.now_playing = Some(track.clone());
        self.pending_seek = start_at;
        self.mark_dirty();
        self.start_stream(track);
    }

    fn start_library_scan(&mut self) {
        if self.library_paths.is_empty() {
            self.flash("set library_paths in config.toml to scan local music");
            return;
        }
        if self.library_scanning {
            return;
        }
        self.library_scanning = true;
        self.status = "scanning local library…".to_string();
        let paths = self.library_paths.clone();
        let tx = self.events_tx.clone();
        let _ = thread::Builder::new()
            .name("library-scan".to_string())
            .spawn(move || {
                let tracks = library::scan(&paths);
                let _ = tx.send(AppEvent::LibraryDone {
                    tracks,
                    error: None,
                });
            });
    }

    fn request_artwork(&mut self, track: &Track) {
        let Some(url) = track.artwork.clone().filter(|url| !url.trim().is_empty()) else {
            return;
        };
        if self.artwork_url.as_deref() == Some(url.as_str()) {
            return;
        }
        self.artwork_url = Some(url.clone());
        let track_id = track.id.clone();
        let tx = self.events_tx.clone();
        let _ = thread::Builder::new()
            .name("artwork".to_string())
            .spawn(move || {
                let result = crate::artwork::load(&url).map_err(|err| err.to_string());
                let _ = tx.send(AppEvent::ArtworkDone { track_id, result });
            });
    }

    fn share_now_playing(&mut self) {
        let Some(track) = self.now_playing.clone() else {
            self.flash("nothing playing to share");
            return;
        };
        let tx = self.events_tx.clone();
        let _ = thread::Builder::new()
            .name("share".to_string())
            .spawn(move || {
                let result = crate::social::share(&track).map_err(|err| err.to_string());
                let _ = tx.send(AppEvent::ShareDone { result });
            });
    }

    fn record_history(&mut self, track: &Track) {
        let now = unix_secs();
        if let Some(last) = self.history.first() {
            if last.track.id == track.id && now.saturating_sub(last.played_at) < 5 {
                return;
            }
        }
        self.history.insert(
            0,
            HistoryEntry {
                track: track.clone(),
                played_at: now,
            },
        );
        self.history.truncate(500);
        self.history_selected = 0;
        self.mark_dirty();
    }

    fn start_stream(&mut self, track: Track) {
        self.start_stream_with(track, false);
    }

    /// Metadata providers in priority order (active first, then the rest), so
    /// a broken provider can fall through to another installed one.
    fn metadata_order(&self) -> Vec<InstalledPlugin> {
        let mut order: Vec<InstalledPlugin> = Vec::new();
        if let Some(active) = self
            .plugins
            .active(PluginKind::Metadata, self.active_metadata.as_deref())
        {
            order.push(active);
        }
        for plugin in &self.plugins.installed {
            if plugin.manifest.kind == PluginKind::Metadata
                && !order
                    .iter()
                    .any(|known| known.manifest.id == plugin.manifest.id)
            {
                order.push(plugin.clone());
            }
        }
        order
    }

    /// Streaming providers in priority order: the user's active plugin first,
    /// then every other installed one. When the active provider hands over a
    /// dead URL or only a 30-second preview clip, we walk down this list
    /// instead of giving up on the track.
    fn streaming_order(&self) -> Vec<InstalledPlugin> {
        let mut order: Vec<InstalledPlugin> = Vec::new();
        if let Some(active) = self
            .plugins
            .active(PluginKind::Streaming, self.active_streaming.as_deref())
        {
            order.push(active);
        }
        for plugin in &self.plugins.installed {
            if plugin.manifest.kind == PluginKind::Streaming
                && !order
                    .iter()
                    .any(|known| known.manifest.id == plugin.manifest.id)
            {
                order.push(plugin.clone());
            }
        }
        order
    }

    fn start_stream_with(&mut self, track: Track, force: bool) {
        if self.audio.is_none() {
            return;
        }
        self.record_history(&track);
        // Offline copies always win: they are instant and immune to CDN 403s.
        if let Some(path) = self.download_path(&track.id).cloned() {
            if path.exists() {
                let info = StreamInfo {
                    url: path.display().to_string(),
                    extractor: Some("offline".to_string()),
                    ..Default::default()
                };
                self.current_stream = Some(info.clone());
                if let Some(audio) = self.audio.as_ref() {
                    let start = self.pending_seek.take().unwrap_or(0.0);
                    if audio.load_from(&info, start).is_ok() {
                        self.status = "playing offline download".to_string();
                        self.mark_dirty();
                        self.prefetch_next();
                        return;
                    }
                }
            }
        }
        if !force {
            self.stream_source = 0;
            self.playback_retries = 0;
            self.playback_failures = 0;
            if let Some((info, at)) = self.stream_cache.get(&track.id) {
                if at.elapsed() < Duration::from_secs(900) {
                    self.current_stream = Some(info.clone());
                    if let Some(audio) = self.audio.as_ref() {
                        let start = self.pending_seek.take().unwrap_or(0.0);
                        if audio.load_from(info, start).is_ok() {
                            let via = info
                                .extractor
                                .clone()
                                .or_else(|| self.active_streaming.clone())
                                .unwrap_or_else(|| "cache".to_string());
                            self.status = format!("playing via {via} · cached");
                            self.mark_dirty();
                            self.prefetch_next();
                            return;
                        }
                    }
                }
            }
            self.current_stream = None;
        }

        self.resolve_track(track, self.stream_source);
    }

    fn resolve_track(&mut self, track: Track, source: usize) {
        let order = self.streaming_order();
        let Some(plugin) = order.get(source).cloned() else {
            self.resolving = false;
            self.flash("no streaming plugin installed — open the plugin store (5)");
            return;
        };
        let source = source.min(order.len().saturating_sub(1));
        self.stream_source = source;
        self.resolving = true;
        self.pending_track_id = Some(track.id.clone());
        self.resolve_started = Some(Instant::now());
        self.status = format!("resolving “{}” via {}…", track.title, plugin.manifest.name);
        let tx = self.events_tx.clone();
        thread::spawn(move || {
            let track_json = serde_json::to_string(&track).unwrap_or_else(|_| "{}".to_string());
            let args = vec!["resolve".to_string(), track_json];
            let result = run_plugin(&plugin, &args, Duration::from_secs(75))
                .and_then(|raw| {
                    let mut info: StreamInfo = serde_json::from_str(&raw)
                        .map_err(|err| anyhow::anyhow!("invalid plugin JSON: {err}"))?;
                    if info.url.trim().is_empty() {
                        return Err(anyhow::anyhow!("plugin returned an empty stream URL"));
                    }
                    if info.url.len() > 16_000 {
                        info.url.truncate(16_000);
                    }
                    Ok(info)
                })
                .map_err(|err| err.to_string());
            let _ = tx.send(AppEvent::StreamResolved {
                track_id: track.id,
                result,
            });
        });
    }

    /// Move down the list of installed streaming providers and resolve again.
    /// Returns true when another attempt was started.
    fn try_alternate_stream(&mut self, track_id: &str, reason: &str) -> bool {
        let order = self.streaming_order();
        let next = self.stream_source + 1;
        if next >= order.len() {
            return false;
        }
        let Some(track) = self
            .now_playing
            .clone()
            .filter(|track| track.id == track_id)
        else {
            return false;
        };
        let via = order[next].manifest.name.clone();
        self.status = format!("{reason} · trying {via}...");
        self.resolve_track(track, next);
        true
    }

    fn try_stream_fallback(&mut self) -> bool {
        let Some(mut stream) = self.current_stream.clone() else {
            return false;
        };
        let Some(audio) = self.audio.as_ref() else {
            return false;
        };
        if let Some(backup) = stream
            .backup_url
            .take()
            .filter(|url| !url.trim().is_empty())
        {
            stream.url = backup;
            self.current_stream = Some(stream.clone());
            if audio.load(&stream).is_ok() {
                self.status = "retrying with an alternate audio source".to_string();
                return true;
            }
        }
        if let Some(webpage) = stream
            .webpage_url
            .clone()
            .filter(|url| !url.trim().is_empty())
        {
            let mut page = stream;
            page.url = webpage;
            page.webpage_url = None;
            page.backup_url = None;
            page.http_headers.clear();
            self.current_stream = Some(page.clone());
            if audio.load(&page).is_ok() {
                self.status = "retrying via the streaming site".to_string();
                return true;
            }
        }
        false
    }

    fn handle_playback_error(&mut self, err: String) {
        self.resolving = false;
        self.pending_track_id = None;
        if self.try_stream_fallback() {
            return;
        }
        if self.playback_retries < 2 {
            self.playback_retries += 1;
            if let Some(track) = self.now_playing.clone() {
                self.flash(format!(
                    "stream failed; re-resolving ({}/2)…",
                    self.playback_retries
                ));
                self.start_stream_with(track, true);
                return;
            }
        }
        self.playback_failures += 1;
        let title = self
            .now_playing
            .as_ref()
            .map(|track| track.title.clone())
            .unwrap_or_else(|| "this track".to_string());
        if self.playback_failures >= 3 || self.queue.len() <= 1 {
            self.flash(format!("playback error: {err}"));
            return;
        }
        self.flash(format!("could not play “{title}”; skipping"));
        self.next_track(true);
    }

    fn on_track_ended(&mut self) {
        self.playback_retries = 0;
        self.playback_failures = 0;
        if self.sleep == SleepMode::EndOfTrack {
            self.sleep = SleepMode::Off;
            self.restore_sleep_volume();
            // Line up the next track so pressing Space later starts there
            // instead of replaying the song the timer just ended.
            let next = self.now_index.and_then(|index| {
                if index + 1 < self.queue.len() {
                    Some(index + 1)
                } else if self.repeat == RepeatMode::All && !self.queue.is_empty() {
                    Some(0)
                } else {
                    None
                }
            });
            if let Some(index) = next {
                self.now_index = Some(index);
                self.queue_selected = index;
                self.now_playing = self.queue.get(index).cloned();
            }
            if let Some(audio) = self.audio.as_ref() {
                let _ = audio.stop();
            }
            self.status = "sleep timer — playback stopped after the track".to_string();
            return;
        }
        if self.repeat == RepeatMode::One {
            if let Some(track) = self.now_playing.clone() {
                self.start_stream_with(track, true);
                return;
            }
        }
        self.next_track(true);
    }

    fn next_track(&mut self, auto: bool) {
        if self.queue.is_empty() {
            if auto {
                self.status = "queue finished".to_string();
            } else {
                self.flash("queue is empty");
            }
            return;
        }

        let Some(index) = self.now_index else {
            self.play_queue_index(0);
            return;
        };
        if index >= self.queue.len() {
            self.play_queue_index(0);
            return;
        }

        let len = self.queue.len();
        let next = if self.shuffle && len > 1 {
            pseudo_random_except(len, index)
        } else if index + 1 < len {
            index + 1
        } else if self.repeat == RepeatMode::All && len > 0 {
            0
        } else {
            if auto && self.radio {
                if self.radio_loading {
                    self.status = "radio: finding more tracks…".to_string();
                } else {
                    self.radio_advance = true;
                    self.start_radio();
                }
            } else if auto {
                self.status = "queue finished · z sleeps · Ctrl+T radio".to_string();
            } else {
                self.flash("end of queue");
            }
            return;
        };
        self.play_queue_index(next);
    }

    fn previous_track(&mut self) {
        let Some(index) = self.now_index else {
            if !self.queue.is_empty() {
                self.play_queue_index(0);
            }
            return;
        };
        if index > 0 {
            self.play_queue_index(index - 1);
            return;
        }
        if self.repeat == RepeatMode::All && self.queue.len() > 1 {
            self.play_queue_index(self.queue.len() - 1);
            return;
        }
        let position = self.audio_snapshot().position;
        if position > 3.0 {
            if let Some(audio) = self.audio.as_ref() {
                let _ = audio.seek_relative(-position);
            }
        } else if let Some(track) = self.now_playing.clone() {
            self.start_stream(track);
        }
    }

    fn toggle_pause(&mut self) {
        let idle = self
            .audio
            .as_ref()
            .map(|audio| audio.snapshot().idle)
            .unwrap_or(false);
        // Nothing loaded (fresh launch with a restored queue, or the queue
        // finished): Space behaves like Spotify and starts playback instead of
        // silently doing nothing.
        if idle && self.resume_queue() {
            return;
        }
        let Some(audio) = self.audio.as_ref() else {
            self.flash("audio engine unavailable");
            return;
        };
        match audio.toggle_pause() {
            Ok(()) => {
                let paused = audio.snapshot().paused;
                self.status = if paused {
                    "paused".to_string()
                } else {
                    "playing".to_string()
                };
            }
            Err(err) => self.flash(format!("pause failed: {err}")),
        }
    }

    fn seek(&mut self, seconds: f64) {
        if let Some(audio) = self.audio.as_ref() {
            match audio.seek_relative(seconds) {
                Ok(()) => {
                    self.status = format!("seek {:+}s", seconds as i64);
                }
                Err(err) => self.flash(format!("seek failed: {err}")),
            }
        }
    }

    fn change_volume(&mut self, delta: f64) {
        self.volume = (self.volume + delta).clamp(0.0, 130.0);
        if self.volume > 0.0 {
            self.last_volume = self.volume;
        }
        self.apply_volume();
    }

    fn toggle_mute(&mut self) {
        if self.volume > 0.0 {
            self.last_volume = self.volume;
            self.volume = 0.0;
        } else {
            self.volume = self.last_volume.max(5.0);
        }
        self.apply_volume();
    }

    fn apply_volume(&mut self) {
        if let Some(audio) = self.audio.as_ref() {
            if let Err(err) = audio.set_volume(self.volume) {
                self.flash(format!("volume failed: {err}"));
                return;
            }
        }
        self.status = format!("volume {:.0}%", self.volume);
        self.mark_dirty();
    }

    fn change_speed(&mut self, delta: f64) {
        let speed = ((self.speed + delta) * 10.0).round() / 10.0;
        self.speed = speed.clamp(0.5, 2.0);
        if let Some(audio) = self.audio.as_ref() {
            if let Err(err) = audio.set_speed(self.speed) {
                self.flash(format!("speed failed: {err}"));
                return;
            }
        }
        self.status = format!("speed {:.1}×", self.speed);
        self.mark_dirty();
    }

    fn reset_speed(&mut self) {
        self.speed = 1.0;
        if let Some(audio) = self.audio.as_ref() {
            let _ = audio.set_speed(self.speed);
        }
        self.status = "speed 1.0×".to_string();
        self.mark_dirty();
    }

    fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
        self.status = if self.shuffle {
            "shuffle on".to_string()
        } else {
            "shuffle off".to_string()
        };
        self.mark_dirty();
    }

    fn cycle_repeat(&mut self) {
        self.repeat = self.repeat.next();
        self.status = self.repeat.label().to_string();
        self.mark_dirty();
    }

    fn toggle_favorite(&mut self) {
        let Some(track) = self.selected_track().or_else(|| self.now_playing.clone()) else {
            self.flash("nothing selected");
            return;
        };
        if let Some(index) = self.favorites.iter().position(|item| item.id == track.id) {
            let removed = self.favorites.remove(index);
            self.push_undo(UndoAction::Favorites {
                track: removed,
                index,
            });
            self.favorites_selected = self
                .favorites_selected
                .min(self.favorites.len().saturating_sub(1));
            self.flash(format!("removed “{}” from favorites", track.title));
        } else {
            self.favorites.push(track.clone());
            self.flash(format!("★ favorited “{}”", track.title));
        }
        self.mark_dirty();
    }

    pub fn is_favorite(&self, id: &str) -> bool {
        self.favorites.iter().any(|track| track.id == id)
    }

    pub fn marked_tracks(&self) -> Vec<Track> {
        let mut tracks: Vec<Track> = self.marked.values().cloned().collect();
        tracks.sort_by_key(|track| track.title.to_lowercase());
        tracks
    }

    fn queue_selected(&mut self) {
        if !self.marked.is_empty() {
            let tracks = self.marked_tracks();
            let before = self.queue.len();
            for track in tracks {
                if !self.queue.iter().any(|item| item.id == track.id) {
                    self.queue.push(track);
                }
            }
            let added = self.queue.len() - before;
            self.flash(format!(
                "queued {added} marked track{}",
                if added == 1 { "" } else { "s" }
            ));
            self.mark_dirty();
            return;
        }
        let Some(track) = self.selected_track() else {
            self.flash("nothing selected");
            return;
        };
        self.add_to_queue(track);
    }

    fn add_to_queue(&mut self, track: Track) {
        if self.queue.iter().any(|item| item.id == track.id) {
            self.flash("already in queue");
            return;
        }
        self.queue.push(track.clone());
        self.flash(format!("queued “{}”", track.title));
        self.mark_dirty();
    }

    pub fn history_tracks(&self) -> Vec<Track> {
        self.history
            .iter()
            .map(|entry| entry.track.clone())
            .collect()
    }

    pub fn current_view_tracks(&self) -> Vec<Track> {
        match self.view {
            View::Search => self.search_results.tracks.clone(),
            View::Favorites => self.favorites.clone(),
            View::Queue => self.queue.clone(),
            View::Playlists => self
                .open_playlist
                .and_then(|index| self.playlists.get(index))
                .map(|playlist| playlist.tracks.clone())
                .unwrap_or_default(),
            View::Browse => self
                .browse_stack
                .last()
                .map(|page| page.tracks.clone())
                .unwrap_or_default(),
            View::Downloads => self.download_tracks(),
            View::Plugins => Vec::new(),
            View::History => self.history_tracks(),
            View::Library => self.library.clone(),
        }
    }

    fn queue_all_current(&mut self) {
        let tracks = self.current_view_tracks();
        if tracks.is_empty() {
            self.flash("nothing to queue here");
            return;
        }
        let before = self.queue.len();
        for track in tracks {
            if !self.queue.iter().any(|item| item.id == track.id) {
                self.queue.push(track);
            }
        }
        let added = self.queue.len() - before;
        self.flash(format!("queued {added} tracks"));
        self.mark_dirty();
    }

    fn remove_current_context(&mut self) {
        match self.view {
            View::Queue => self.remove_queue_selected(),
            View::Favorites => {
                if let Some(track) = self.favorites.get(self.favorites_selected).cloned() {
                    if let Some(index) = self.favorites.iter().position(|item| item.id == track.id)
                    {
                        self.push_undo(UndoAction::Favorites {
                            track: track.clone(),
                            index,
                        });
                    }
                    self.favorites.retain(|item| item.id != track.id);
                    self.favorites_selected = self
                        .favorites_selected
                        .min(self.favorites.len().saturating_sub(1));
                    self.flash(format!("removed “{}” from favorites", track.title));
                    self.mark_dirty();
                }
            }
            View::Playlists => {
                if let Some(playlist_index) = self.open_playlist {
                    let removed = self.playlists.get_mut(playlist_index).and_then(|playlist| {
                        if self.playlist_track_selected < playlist.tracks.len() {
                            let index = self.playlist_track_selected;
                            let track = playlist.tracks.remove(index);
                            let remaining = playlist.tracks.len();
                            Some((track, index, remaining))
                        } else {
                            None
                        }
                    });
                    if let Some((track, index, remaining)) = removed {
                        self.push_undo(UndoAction::Playlist {
                            playlist: playlist_index,
                            track: track.clone(),
                            index,
                        });
                        self.playlist_track_selected = self
                            .playlist_track_selected
                            .min(remaining.saturating_sub(1));
                        self.flash(format!("removed “{}” from playlist", track.title));
                        self.mark_dirty();
                    }
                }
            }
            View::Downloads => self.remove_download_selected(),
            _ => self.flash("nothing to remove here"),
        }
    }

    fn remove_queue_selected(&mut self) {
        if self.queue.is_empty() {
            return;
        }
        let index = self.queue_selected.min(self.queue.len() - 1);
        let was_current = self.now_index == Some(index);
        let removed = self.queue.remove(index);
        self.push_undo(UndoAction::Queue {
            track: removed.clone(),
            index,
        });
        if let Some(now) = self.now_index {
            if index < now {
                self.now_index = Some(now - 1);
            } else if index == now {
                self.now_index = None;
            }
        }
        self.queue_selected = self.queue_selected.min(self.queue.len().saturating_sub(1));
        self.flash(format!("removed “{}” from queue", removed.title));
        if was_current {
            if self.queue.is_empty() {
                self.now_playing = None;
                self.resolving = false;
                self.pending_track_id = None;
                if let Some(audio) = self.audio.as_ref() {
                    let _ = audio.stop();
                }
            } else {
                let next = index.min(self.queue.len() - 1);
                self.play_queue_index(next);
            }
        }
        self.mark_dirty();
    }

    fn move_queue(&mut self, delta: isize) {
        if self.queue.len() < 2 {
            return;
        }
        let from = self.queue_selected.min(self.queue.len() - 1);
        let to = if delta < 0 {
            from.saturating_sub(delta.unsigned_abs())
        } else {
            (from + delta as usize).min(self.queue.len() - 1)
        };
        if from == to {
            return;
        }
        self.queue.swap(from, to);
        if let Some(now) = self.now_index {
            if now == from {
                self.now_index = Some(to);
            } else if now == to {
                self.now_index = Some(from);
            }
        }
        self.queue_selected = to;
        self.mark_dirty();
    }

    fn activate_selected(&mut self) {
        match self.view {
            View::Search => self.play_selected_search(),
            View::Browse => self.activate_browse(),
            View::Queue => {
                if self.queue_selected < self.queue.len() {
                    self.play_queue_index(self.queue_selected);
                }
            }
            View::Favorites => {
                if let Some(track) = self.favorites.get(self.favorites_selected).cloned() {
                    let context = self.favorites.clone();
                    self.play_with_context(track, context);
                }
            }
            View::Playlists => {
                if let Some(playlist_index) = self.open_playlist {
                    let queued = self
                        .playlists
                        .get(playlist_index)
                        .and_then(|playlist| playlist.tracks.get(self.playlist_track_selected))
                        .cloned();
                    if let Some(track) = queued {
                        // Autoplay through the rest of the playlist.
                        let context = self
                            .playlists
                            .get(playlist_index)
                            .map(|playlist| playlist.tracks.clone())
                            .unwrap_or_default();
                        self.play_with_context(track, context);
                    }
                } else if self.playlist_selected < self.playlists.len() {
                    self.open_playlist = Some(self.playlist_selected);
                    self.playlist_track_selected = 0;
                }
            }
            View::Downloads => {
                let tracks = self.download_tracks();
                if let Some(track) = tracks.get(self.downloads_selected).cloned() {
                    self.play_with_context(track, tracks);
                }
            }
            View::History => {
                if let Some(entry) = self.history.get(self.history_selected) {
                    let track = entry.track.clone();
                    let context = self.history_tracks();
                    self.play_with_context(track, context);
                }
            }
            View::Library => {
                if let Some(track) = self.library.get(self.library_selected).cloned() {
                    let context = self.library.clone();
                    self.play_with_context(track, context);
                }
            }
            View::Plugins => self.toggle_selected_plugin(),
        }
    }

    fn open_artist(&mut self, artist: Artist) {
        self.browse_generation = self.browse_generation.wrapping_add(1);
        let generation = self.browse_generation;
        let page = BrowsePage {
            kind: BrowseKind::Artist,
            title: artist.name.clone(),
            subtitle: format!("artist · {}", artist.source),
            artwork: artist.artwork.clone(),
            source: artist.source.clone(),
            albums: Vec::new(),
            tracks: Vec::new(),
            selected: 0,
            loading: true,
            error: None,
            generation,
        };
        self.browse_stack.push(page);
        self.view = View::Browse;
        self.search_focused = false;
        self.start_browse_request(generation, "artist", artist.id);
    }

    fn open_album(&mut self, album: Album) {
        self.browse_generation = self.browse_generation.wrapping_add(1);
        let generation = self.browse_generation;
        // Artist pages only attach the songs that showed up in their top-track
        // list, so an album opened from there is usually incomplete. Treat it
        // as loaded only when the track count lines up with the provider's.
        let has_tracks = !album.tracks.is_empty()
            && album
                .track_count
                .is_none_or(|count| album.tracks.len() as u64 >= count);
        let tracks = if has_tracks {
            album.tracks.clone()
        } else {
            Vec::new()
        };
        let page = BrowsePage {
            kind: BrowseKind::Album,
            title: album.title.clone(),
            subtitle: format!("album · {}", album.artist),
            artwork: album.artwork.clone(),
            source: album.source.clone(),
            albums: vec![album.clone()],
            tracks,
            selected: 0,
            loading: !has_tracks,
            error: None,
            generation,
        };
        self.browse_stack.push(page);
        self.view = View::Browse;
        self.search_focused = false;
        if !has_tracks {
            self.start_browse_request(generation, "album", album.id);
        } else {
            self.status = format!("{} · album", album.title);
        }
    }

    fn start_browse_request(&mut self, generation: u64, command: &str, argument: String) {
        let order = self.metadata_order();
        let Some(primary) = order.first() else {
            if let Some(page) = self.browse_stack.last_mut() {
                page.loading = false;
                page.error = Some("no metadata plugin installed".to_string());
            }
            return;
        };
        self.status = format!("loading {} via {}…", command, primary.manifest.name);
        let tx = self.events_tx.clone();
        let command = command.to_string();
        thread::spawn(move || {
            let mut errors = Vec::new();
            for plugin in order {
                let name = plugin.manifest.name.clone();
                let args = vec![command.clone(), argument.clone()];
                match run_plugin(&plugin, &args, Duration::from_secs(25)).and_then(|raw| {
                    let payload: SearchPayload = serde_json::from_str(&raw)
                        .map_err(|err| anyhow::anyhow!("invalid plugin JSON: {err}"))?;
                    Ok(payload.into_results())
                }) {
                    Ok(results) => {
                        let _ = tx.send(AppEvent::BrowseDone {
                            generation,
                            result: Ok(results),
                        });
                        return;
                    }
                    Err(err) => errors.push(format!("{name}: {err}")),
                }
            }
            let _ = tx.send(AppEvent::BrowseDone {
                generation,
                result: Err(errors.join(" · ")),
            });
        });
    }

    fn browse_back(&mut self) {
        self.browse_stack.pop();
        if self.browse_stack.is_empty() {
            self.view = View::Search;
        }
    }

    fn activate_browse(&mut self) {
        let Some(page) = self.browse_stack.last() else {
            self.view = View::Search;
            return;
        };
        match browse_selectable(page).get(page.selected).copied() {
            Some(BrowseRow::Album(index)) => {
                if let Some(album) = page.albums.get(index).cloned() {
                    self.open_album(album);
                }
            }
            Some(BrowseRow::Track(index)) => {
                if let Some(track) = page.tracks.get(index).cloned() {
                    // Album / artist pages: play from here, keep going.
                    let context = page.tracks.clone();
                    self.play_with_context(track, context);
                }
            }
            _ => {}
        }
    }

    fn move_playlist_track(&mut self, delta: isize) {
        let Some(index) = self.open_playlist else {
            return;
        };
        let Some(playlist) = self.playlists.get_mut(index) else {
            return;
        };
        if playlist.tracks.len() < 2 {
            return;
        }
        let from = self.playlist_track_selected.min(playlist.tracks.len() - 1);
        let to = if delta < 0 {
            from.saturating_sub(delta.unsigned_abs())
        } else {
            (from + delta as usize).min(playlist.tracks.len() - 1)
        };
        if from == to {
            return;
        }
        playlist.tracks.swap(from, to);
        self.playlist_track_selected = to;
        self.mark_dirty();
    }

    /// Shared by `h`, `Alt+←` and `Backspace`: leave the current page.
    fn history_back(&mut self) {
        match self.view {
            View::Browse => self.browse_back(),
            View::Playlists if self.open_playlist.is_some() => self.open_playlist = None,
            _ => {}
        }
    }

    /// `N`: jump the selected track to the front of the queue (Spotify's
    /// "Play next").
    fn play_selected_next(&mut self) {
        let Some(track) = self.selected_track() else {
            self.flash("nothing selected");
            return;
        };
        if self.now_playing.is_none() {
            self.play_track_now(track);
            return;
        }
        if self
            .now_playing
            .as_ref()
            .is_some_and(|playing| playing.id == track.id)
        {
            self.flash("it is playing right now");
            return;
        }
        if let Some(index) = self.queue.iter().position(|item| item.id == track.id) {
            self.queue.remove(index);
            if index < self.now_index.unwrap_or(usize::MAX) {
                if let Some(now) = self.now_index {
                    self.now_index = Some(now.saturating_sub(1));
                }
            }
        }
        let insert_at = self
            .now_index
            .map(|index| index + 1)
            .unwrap_or(0)
            .min(self.queue.len());
        self.queue.insert(insert_at, track.clone());
        self.queue_selected = insert_at;
        self.flash(format!("“{}” will play next", track.title));
        self.mark_dirty();
    }

    fn begin_add_to_playlist(&mut self) {
        let tracks = if self.marked.is_empty() {
            self.context_tracks()
        } else {
            self.marked_tracks()
        };
        if tracks.is_empty() {
            self.flash("nothing selected");
            return;
        }
        self.pending_playlist_tracks = tracks;
        self.overlay = Some(Overlay::PlaylistPicker { selected: 0 });
    }

    fn context_tracks(&self) -> Vec<Track> {
        match self.view {
            View::Search => match self.search_selected_row() {
                Some(SearchRow::Track(index)) => self
                    .search_results
                    .tracks
                    .get(index)
                    .cloned()
                    .map(|track| vec![track])
                    .unwrap_or_default(),
                Some(SearchRow::Album(index)) => {
                    let Some(album) = self.search_results.albums.get(index) else {
                        return Vec::new();
                    };
                    if album.tracks.is_empty() {
                        self.search_results
                            .tracks
                            .iter()
                            .filter(|track| {
                                track.album.eq_ignore_ascii_case(&album.title)
                                    && album.artist.eq_ignore_ascii_case(&track.artist)
                            })
                            .cloned()
                            .collect()
                    } else {
                        album.tracks.clone()
                    }
                }
                Some(SearchRow::Artist(index)) => {
                    let Some(artist) = self.search_results.artists.get(index) else {
                        return Vec::new();
                    };
                    self.search_results
                        .tracks
                        .iter()
                        .filter(|track| track.artist.eq_ignore_ascii_case(&artist.name))
                        .cloned()
                        .collect()
                }
                _ => Vec::new(),
            },
            View::Browse => {
                let Some(page) = self.browse_stack.last() else {
                    return Vec::new();
                };
                match browse_selectable(page).get(page.selected) {
                    Some(BrowseRow::Track(index)) => page
                        .tracks
                        .get(*index)
                        .cloned()
                        .map(|track| vec![track])
                        .unwrap_or_default(),
                    Some(BrowseRow::Album(index)) => {
                        let Some(album) = page.albums.get(*index) else {
                            return Vec::new();
                        };
                        if album.tracks.is_empty() {
                            page.tracks
                                .iter()
                                .filter(|track| {
                                    track.album.eq_ignore_ascii_case(&album.title)
                                        && album.artist.eq_ignore_ascii_case(&track.artist)
                                })
                                .cloned()
                                .collect()
                        } else {
                            album.tracks.clone()
                        }
                    }
                    _ => Vec::new(),
                }
            }
            _ => self
                .selected_track()
                .map(|track| vec![track])
                .unwrap_or_default(),
        }
    }

    fn add_tracks_to_playlist(&mut self, index: usize, tracks: Vec<Track>) {
        let Some(playlist) = self.playlists.get_mut(index) else {
            return;
        };
        let name = playlist.name.clone();
        let mut added = 0usize;
        for track in tracks {
            if !playlist.tracks.iter().any(|item| item.id == track.id) {
                playlist.tracks.push(track);
                added += 1;
            }
        }
        if added == 0 {
            self.flash(format!("already in “{name}”"));
            return;
        }
        self.flash(format!(
            "added {added} track{} to “{name}”",
            if added == 1 { "" } else { "s" }
        ));
        self.mark_dirty();
    }

    fn create_playlist(&mut self, name: &str, tracks: Vec<Track>) {
        let name = name.trim();
        if name.is_empty() {
            self.flash("playlist name cannot be empty");
            if !tracks.is_empty() {
                self.pending_playlist_tracks = tracks;
            }
            self.overlay = Some(Overlay::NewPlaylist {
                name: name.to_string(),
            });
            return;
        }
        let id = format!("playlist-{}", unix_nanos());
        let playlist = Playlist {
            id,
            name: name.to_string(),
            tracks,
        };
        let count = playlist.tracks.len();
        self.playlists.push(playlist);
        self.playlist_selected = self.playlists.len().saturating_sub(1);
        self.overlay = None;
        self.flash(format!(
            "created playlist “{name}”{}",
            if count > 0 {
                format!(" with {count} track{}", if count == 1 { "" } else { "s" })
            } else {
                String::new()
            }
        ));
        self.mark_dirty();
    }

    fn play_playlist(&mut self, index: usize) {
        let Some(playlist) = self.playlists.get(index) else {
            return;
        };
        let playlist_name = playlist.name.clone();
        let tracks = playlist.tracks.clone();
        let Some(first) = tracks.first().cloned() else {
            self.flash("playlist is empty");
            return;
        };
        // Same autoplay rules as pressing Enter on a song: line the rest of
        // the playlist up behind it and drop stale future tracks.
        self.play_with_context(first, tracks);
        self.flash(format!("playing playlist “{playlist_name}”"));
    }

    /// `t` in the queue: jump the cursor to whatever is playing right now.
    fn jump_to_now_playing(&mut self) {
        match self.now_index {
            Some(index) if index < self.queue.len() => {
                self.queue_selected = index;
                self.flash("jumped to the current track");
            }
            _ => self.flash("nothing is playing right now"),
        }
    }

    /// `p` on an album/artist page: play the page from the top and keep going.
    fn play_browse_page(&mut self) {
        let Some(page) = self.browse_stack.last() else {
            return;
        };
        let Some(first) = page.tracks.first().cloned() else {
            self.flash("nothing to play on this page");
            return;
        };
        let tracks = page.tracks.clone();
        let title = page.title.clone();
        self.play_with_context(first, tracks);
        self.flash(format!("playing “{title}”"));
    }

    fn run_confirm(&mut self, action: ConfirmAction) {
        match action {
            ConfirmAction::DeletePlaylist(index) => {
                if index < self.playlists.len() {
                    let playlist = self.playlists.remove(index);
                    self.playlist_selected = self
                        .playlist_selected
                        .min(self.playlists.len().saturating_sub(1));
                    if self.open_playlist == Some(index) {
                        self.open_playlist = None;
                    } else if self.open_playlist.is_some_and(|open| open > index) {
                        self.open_playlist = self.open_playlist.map(|open| open - 1);
                    }
                    self.flash(format!("deleted playlist “{}”", playlist.name));
                    self.mark_dirty();
                }
            }
            ConfirmAction::ClearQueue => {
                self.queue.clear();
                self.now_index = None;
                self.queue_selected = 0;
                if let Some(audio) = self.audio.as_ref() {
                    let _ = audio.stop();
                }
                self.now_playing = None;
                self.resolving = false;
                self.pending_track_id = None;
                self.flash("queue cleared");
                self.mark_dirty();
            }
        }
    }

    fn open_filter_or_search(&mut self) {
        if self.view == View::Search {
            // `/` always lands in the field; Esc gets you out.
            self.search_focused = true;
            return;
        }
        self.open_filter_current();
    }

    fn open_filter_current(&mut self) {
        self.search_focused = false;
        let kind = match self.view {
            View::Search => FilterKind::Search,
            View::Queue => FilterKind::Queue,
            View::Favorites => FilterKind::Favorites,
            View::Playlists => {
                if let Some(index) = self.open_playlist {
                    FilterKind::Playlist(index)
                } else {
                    self.flash("open a playlist first");
                    return;
                }
            }
            View::Downloads => {
                self.flash("downloads are already a short list — use j/k");
                return;
            }
            View::Plugins => {
                self.open_command_palette();
                return;
            }
            View::History => {
                self.flash("history is already newest-first — use j/k");
                return;
            }
            View::Library => FilterKind::Library,
            View::Browse => {
                self.flash("go back to search to filter results");
                return;
            }
        };
        if self.filter_source(kind).is_empty() {
            self.flash("nothing to filter here");
            return;
        }
        self.overlay = Some(Overlay::Filter {
            kind,
            query: String::new(),
            selected: 0,
        });
    }

    pub fn filter_source(&self, kind: FilterKind) -> &[Track] {
        match kind {
            FilterKind::Search => &self.search_results.tracks,
            FilterKind::Queue => &self.queue,
            FilterKind::Favorites => &self.favorites,
            FilterKind::Playlist(index) => self
                .playlists
                .get(index)
                .map(|playlist| playlist.tracks.as_slice())
                .unwrap_or(&[]),
            FilterKind::Library => &self.library,
        }
    }

    pub fn filter_ranked(&self, kind: FilterKind, query: &str) -> Vec<usize> {
        let source = self.filter_source(kind);
        let labels: Vec<String> = source.iter().map(Track::search_text).collect();
        fuzzy_rank(query, &labels)
    }

    pub fn filter_track(&self, kind: FilterKind, query: &str, selected: usize) -> Option<Track> {
        let ranked = self.filter_ranked(kind, query);
        ranked
            .get(selected)
            .and_then(|index| self.filter_source(kind).get(*index))
            .cloned()
    }

    fn open_command_palette(&mut self) {
        self.overlay = Some(Overlay::CommandPalette {
            query: String::new(),
            selected: 0,
        });
    }

    fn execute_palette_action(&mut self, action: PaletteAction) {
        match action {
            PaletteAction::PlaySelected => self.activate_selected(),
            PaletteAction::QueueSelected => self.queue_selected(),
            PaletteAction::PlayPause => self.toggle_pause(),
            PaletteAction::NextTrack => self.next_track(false),
            PaletteAction::PreviousTrack => self.previous_track(),
            PaletteAction::SeekForward => self.seek(self.config.seek_step),
            PaletteAction::SeekBackward => self.seek(-self.config.seek_step),
            PaletteAction::SeekForwardLong => self.seek(self.config.long_seek_step),
            PaletteAction::SeekBackwardLong => self.seek(-self.config.long_seek_step),
            PaletteAction::VolumeUp => self.change_volume(self.config.volume_step),
            PaletteAction::VolumeDown => self.change_volume(-self.config.volume_step),
            PaletteAction::ToggleMute => self.toggle_mute(),
            PaletteAction::ToggleFavorite => self.toggle_favorite(),
            PaletteAction::ShowLyrics => self.open_lyrics(),
            PaletteAction::JumpToNowPlaying => {
                self.switch_view(View::Queue);
                self.jump_to_now_playing();
            }
            PaletteAction::PlayPage => {
                if self.view == View::Browse {
                    self.play_browse_page();
                } else {
                    self.flash("open an album or artist page first");
                }
            }
            PaletteAction::FocusSearch => {
                self.switch_view(View::Search);
                self.search_focused = true;
            }
            PaletteAction::GoBack => self.history_back(),
            PaletteAction::PlayNext => self.play_selected_next(),
            PaletteAction::RenamePlaylist => {
                if self.view == View::Playlists {
                    let index = self.open_playlist.unwrap_or(self.playlist_selected);
                    if let Some(playlist) = self.playlists.get(index) {
                        self.overlay = Some(Overlay::RenamePlaylist {
                            index,
                            name: playlist.name.clone(),
                        });
                    } else {
                        self.flash("no playlist here yet");
                    }
                } else {
                    self.switch_view(View::Playlists);
                    self.flash("select a playlist and press e to rename it");
                }
            }
            PaletteAction::ViewSearch => self.switch_view(View::Search),
            PaletteAction::ViewQueue => self.switch_view(View::Queue),
            PaletteAction::ViewFavorites => self.switch_view(View::Favorites),
            PaletteAction::ViewPlaylists => self.switch_view(View::Playlists),
            PaletteAction::ViewDownloads => self.switch_view(View::Downloads),
            PaletteAction::ViewPlugins => self.switch_view(View::Plugins),
            PaletteAction::ViewHistory => self.switch_view(View::History),
            PaletteAction::ViewLibrary => self.switch_view(View::Library),
            PaletteAction::RescanLibrary => self.start_library_scan(),
            PaletteAction::ShareNowPlaying => self.share_now_playing(),
            PaletteAction::ShowStats => self.overlay = Some(Overlay::Stats),
            PaletteAction::NewPlaylist => {
                self.pending_playlist_tracks.clear();
                self.overlay = Some(Overlay::NewPlaylist {
                    name: String::new(),
                });
            }
            PaletteAction::AddToPlaylist => self.begin_add_to_playlist(),
            PaletteAction::ToggleShuffle => self.toggle_shuffle(),
            PaletteAction::CycleRepeat => self.cycle_repeat(),
            PaletteAction::ClearQueue => {
                if !self.queue.is_empty() {
                    self.overlay = Some(Overlay::Confirm {
                        message: "Clear the whole queue and stop playback?".to_string(),
                        action: ConfirmAction::ClearQueue,
                    });
                } else {
                    self.flash("queue is already empty");
                }
            }
            PaletteAction::ClearUpcoming => self.clear_upcoming(),
            PaletteAction::Undo => self.undo_last(),
            PaletteAction::SaveQueue => self.save_queue_as_playlist(),
            PaletteAction::ExportPlaylist => self.export_playlists(),
            PaletteAction::ImportPlaylist => self.import_playlists(),
            PaletteAction::DownloadSelected => self.download_selected(),
            PaletteAction::ToggleRadio => self.toggle_radio(),
            PaletteAction::SleepTimer => self.open_sleep_timer(),
            PaletteAction::ToggleNormalize => self.toggle_normalize(),
            PaletteAction::CycleEq => self.cycle_eq(),
            PaletteAction::CycleTheme => self.cycle_theme(),
            PaletteAction::ImportLink => self.open_link_import(),
            PaletteAction::ToggleCava => self.toggle_cava(),
            PaletteAction::Background => self.detach_background(),
            PaletteAction::Help => self.overlay = Some(Overlay::Help),
            PaletteAction::Quit => self.should_quit = true,
        }
    }

    fn open_lyrics(&mut self) {
        let Some(track) = self.selected_track().or_else(|| self.now_playing.clone()) else {
            self.flash("nothing selected");
            return;
        };

        if let Some(doc) = self.lyrics_cache.get(&track.id).cloned() {
            self.overlay = Some(Overlay::Lyrics(Box::new(LyricsOverlay {
                track,
                lines: parse_lyrics(&doc),
                doc: Some(doc),
                loading: false,
                error: None,
                scroll: 0,
                follow: true,
            })));
            return;
        }

        let Some(plugin) = self
            .plugins
            .active(PluginKind::Lyrics, self.active_lyrics.as_deref())
        else {
            self.overlay = Some(Overlay::Lyrics(Box::new(LyricsOverlay {
                track,
                doc: None,
                lines: Vec::new(),
                loading: false,
                error: Some("no lyrics plugin installed".to_string()),
                scroll: 0,
                follow: true,
            })));
            return;
        };

        self.overlay = Some(Overlay::Lyrics(Box::new(LyricsOverlay {
            track: track.clone(),
            doc: None,
            lines: Vec::new(),
            loading: true,
            error: None,
            scroll: 0,
            follow: true,
        })));

        let tx = self.events_tx.clone();
        thread::spawn(move || {
            let track_json = serde_json::to_string(&track).unwrap_or_else(|_| "{}".to_string());
            let args = vec!["fetch".to_string(), track_json];
            let result = run_plugin(&plugin, &args, Duration::from_secs(35))
                .and_then(|raw| {
                    serde_json::from_str::<LyricsDoc>(&raw)
                        .map_err(|err| anyhow::anyhow!("invalid plugin JSON: {err}"))
                })
                .map_err(|err| err.to_string());
            let _ = tx.send(AppEvent::LyricsDone {
                track_id: track.id,
                result,
            });
        });
    }

    fn update_lyrics_follow(&mut self) {
        let position = self.audio_snapshot().position;
        let now_id = self.now_playing.as_ref().map(|track| track.id.clone());
        let Some(Overlay::Lyrics(lyrics)) = self.overlay.as_mut() else {
            return;
        };
        if !lyrics.follow || lyrics.loading {
            return;
        }
        if now_id.as_deref() != Some(lyrics.track.id.as_str()) {
            return;
        }
        let current = lyrics
            .lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| line.at.map(|at| (index, at)))
            .take_while(|(_, at)| *at <= position + 0.2)
            .last()
            .map(|(index, _)| index)
            .unwrap_or(0);
        if current >= 7 {
            lyrics.scroll = current - 7;
        } else {
            lyrics.scroll = 0;
        }
    }

    pub fn lyrics_current_line(&self) -> Option<usize> {
        let Some(Overlay::Lyrics(lyrics)) = self.overlay.as_ref() else {
            return None;
        };
        if !self
            .now_playing
            .as_ref()
            .is_some_and(|track| track.id == lyrics.track.id)
        {
            return None;
        }
        let position = self.audio_snapshot().position;
        lyrics
            .lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| line.at.map(|at| (index, at)))
            .take_while(|(_, at)| *at <= position + 0.2)
            .last()
            .map(|(index, _)| index)
    }

    fn detach_background(&mut self) {
        if self.audio.is_some() {
            if let Err(err) = crate::daemon::spawn_background() {
                self.flash(format!("could not start background player: {err}"));
                return;
            }
            if let Some(audio) = self.audio.as_mut() {
                audio.detach();
            }
            self.detached_on_quit = true;
            self.should_quit = true;
            self.persist();
        } else {
            self.flash("audio engine is not running");
        }
    }

    /// Called by the headless daemon once it has attached to mpv: if the last
    /// track ended during the handoff, pick the queue back up instead of
    /// sitting there looking busy.
    pub fn daemon_resume_if_idle(&mut self) {
        let idle = self
            .audio
            .as_ref()
            .map(|audio| audio.snapshot().idle)
            .unwrap_or(false);
        if idle && !self.queue.is_empty() {
            self.resume_queue();
        }
    }

    /// Hand mpv over to a new TUI without killing the music.
    pub fn release_audio(&mut self) {
        if let Some(audio) = self.audio.as_mut() {
            audio.release();
        }
    }

    fn switch_view(&mut self, view: View) {
        self.view = view;
        self.search_focused = false;
    }

    fn next_view(&mut self) {
        if self.view == View::Browse {
            self.switch_view(View::Search);
            return;
        }
        let index = View::ALL
            .iter()
            .position(|view| *view == self.view)
            .unwrap_or(0);
        self.switch_view(View::ALL[(index + 1) % View::ALL.len()]);
    }

    fn previous_view(&mut self) {
        if self.view == View::Browse {
            self.switch_view(View::Search);
            return;
        }
        let index = View::ALL
            .iter()
            .position(|view| *view == self.view)
            .unwrap_or(0);
        self.switch_view(View::ALL[(index + View::ALL.len() - 1) % View::ALL.len()]);
    }

    fn move_selection(&mut self, delta: isize) {
        match self.view {
            View::Search => {
                self.search_selected =
                    step_index(self.search_selected, delta, self.search_row_count());
            }
            View::Browse => {
                if let Some(page) = self.browse_stack.last_mut() {
                    let len = browse_selectable(page).len();
                    page.selected = step_index(page.selected, delta, len);
                }
            }
            View::Queue => {
                self.queue_selected = step_index(self.queue_selected, delta, self.queue.len());
            }
            View::Favorites => {
                self.favorites_selected =
                    step_index(self.favorites_selected, delta, self.favorites.len());
            }
            View::Playlists => {
                if self.open_playlist.is_some() {
                    let len = self
                        .open_playlist
                        .and_then(|index| self.playlists.get(index))
                        .map(|playlist| playlist.tracks.len())
                        .unwrap_or(0);
                    self.playlist_track_selected =
                        step_index(self.playlist_track_selected, delta, len);
                } else {
                    self.playlist_selected =
                        step_index(self.playlist_selected, delta, self.playlists.len());
                }
            }
            View::Downloads => {
                self.downloads_selected =
                    step_index(self.downloads_selected, delta, self.download_tracks().len());
            }
            View::Plugins => {
                self.plugin_selected =
                    step_index(self.plugin_selected, delta, self.plugin_entries().len());
            }
            View::History => {
                self.history_selected =
                    step_index(self.history_selected, delta, self.history.len());
            }
            View::Library => {
                self.library_selected =
                    step_index(self.library_selected, delta, self.library.len());
            }
        }
    }

    fn goto_top(&mut self) {
        match self.view {
            View::Search => self.search_selected = 0,
            View::Browse => {
                if let Some(page) = self.browse_stack.last_mut() {
                    page.selected = 0;
                }
            }
            View::Queue => self.queue_selected = 0,
            View::Favorites => self.favorites_selected = 0,
            View::Playlists => {
                if self.open_playlist.is_some() {
                    self.playlist_track_selected = 0;
                } else {
                    self.playlist_selected = 0;
                }
            }
            View::Downloads => self.downloads_selected = 0,
            View::Plugins => self.plugin_selected = 0,
            View::History => self.history_selected = 0,
            View::Library => self.library_selected = 0,
        }
    }

    fn goto_bottom(&mut self) {
        match self.view {
            View::Search => self.search_selected = self.search_row_count().saturating_sub(1),
            View::Browse => {
                if let Some(page) = self.browse_stack.last_mut() {
                    page.selected = browse_selectable(page).len().saturating_sub(1);
                }
            }
            View::Queue => self.queue_selected = self.queue.len().saturating_sub(1),
            View::Favorites => self.favorites_selected = self.favorites.len().saturating_sub(1),
            View::Playlists => {
                if let Some(index) = self.open_playlist {
                    self.playlist_track_selected = self
                        .playlists
                        .get(index)
                        .map(|playlist| playlist.tracks.len().saturating_sub(1))
                        .unwrap_or(0);
                } else {
                    self.playlist_selected = self.playlists.len().saturating_sub(1);
                }
            }
            View::Downloads => {
                self.downloads_selected = self.download_tracks().len().saturating_sub(1)
            }
            View::Plugins => self.plugin_selected = self.plugin_entries().len().saturating_sub(1),
            View::History => self.history_selected = self.history.len().saturating_sub(1),
            View::Library => self.library_selected = self.library.len().saturating_sub(1),
        }
    }

    pub fn scrobbling(&self) -> bool {
        self.scrobbler.enabled()
    }

    pub fn selected_track(&self) -> Option<Track> {
        match self.view {
            View::Search => match self.search_selected_row() {
                Some(SearchRow::Track(index)) => self.search_results.tracks.get(index).cloned(),
                Some(SearchRow::Album(index)) => self
                    .search_results
                    .albums
                    .get(index)
                    .and_then(|album| album.tracks.first().cloned()),
                Some(SearchRow::Artist(index)) => {
                    self.search_results.artists.get(index).and_then(|artist| {
                        self.search_results
                            .tracks
                            .iter()
                            .find(|track| track.artist.eq_ignore_ascii_case(&artist.name))
                            .cloned()
                    })
                }
                _ => None,
            },
            View::Browse => {
                let page = self.browse_stack.last()?;
                match browse_selectable(page).get(page.selected) {
                    Some(BrowseRow::Track(index)) => page.tracks.get(*index).cloned(),
                    Some(BrowseRow::Album(index)) => page
                        .albums
                        .get(*index)
                        .and_then(|album| album.tracks.first().cloned()),
                    _ => None,
                }
            }
            View::Queue => self.queue.get(self.queue_selected).cloned(),
            View::Favorites => self.favorites.get(self.favorites_selected).cloned(),
            View::Playlists => self
                .open_playlist
                .and_then(|index| self.playlists.get(index))
                .and_then(|playlist| playlist.tracks.get(self.playlist_track_selected))
                .cloned(),
            View::Downloads => self.download_tracks().get(self.downloads_selected).cloned(),
            View::History => self
                .history
                .get(self.history_selected)
                .map(|entry| entry.track.clone()),
            View::Library => self.library.get(self.library_selected).cloned(),
            View::Plugins => None,
        }
    }

    pub fn search_row_count(&self) -> usize {
        search_selectable(&self.search_results).len()
    }

    pub fn search_selected_row(&self) -> Option<SearchRow> {
        search_selectable(&self.search_results)
            .get(self.search_selected)
            .copied()
    }

    pub fn search_display_rows(&self) -> Vec<SearchRow> {
        search_display(&self.search_results)
    }

    pub fn browse_page(&self) -> Option<&BrowsePage> {
        self.browse_stack.last()
    }

    pub fn browse_display_rows(&self) -> Vec<BrowseRow> {
        self.browse_stack
            .last()
            .map(browse_display)
            .unwrap_or_default()
    }

    pub fn plugin_entries(&self) -> Vec<PluginEntry> {
        let mut entries = Vec::new();
        for bundled in &self.plugins.catalog {
            let manifest = &bundled.manifest;
            entries.push(PluginEntry {
                id: manifest.id.clone(),
                name: manifest.name.clone(),
                version: manifest.version.clone(),
                kind: manifest.kind,
                description: manifest.description.clone(),
                installed: self.plugins.is_installed(&manifest.id),
                active: self.active_id(manifest.kind).as_deref() == Some(manifest.id.as_str()),
                bundled: true,
            });
        }
        for plugin in &self.plugins.installed {
            if self.plugins.bundled(&plugin.manifest.id).is_some() {
                continue;
            }
            entries.push(PluginEntry {
                id: plugin.manifest.id.clone(),
                name: plugin.manifest.name.clone(),
                version: plugin.manifest.version.clone(),
                kind: plugin.manifest.kind,
                description: plugin.manifest.description.clone(),
                installed: true,
                active: self.active_id(plugin.manifest.kind).as_deref()
                    == Some(plugin.manifest.id.as_str()),
                bundled: false,
            });
        }
        entries
    }

    fn active_id(&self, kind: PluginKind) -> Option<String> {
        match kind {
            PluginKind::Metadata => self.active_metadata.clone(),
            PluginKind::Streaming => self.active_streaming.clone(),
            PluginKind::Lyrics => self.active_lyrics.clone(),
        }
    }

    fn set_active_id(&mut self, kind: PluginKind, id: Option<String>) {
        match kind {
            PluginKind::Metadata => self.active_metadata = id,
            PluginKind::Streaming => self.active_streaming = id,
            PluginKind::Lyrics => self.active_lyrics = id,
        }
    }

    fn toggle_selected_plugin(&mut self) {
        let Some(entry) = self.plugin_entries().get(self.plugin_selected).cloned() else {
            return;
        };
        if !entry.installed {
            match self.plugins.install(&entry.id) {
                Ok(()) => {
                    self.flash(format!("installed {}", entry.name));
                    self.mark_dirty();
                }
                Err(err) => {
                    self.flash(format!("install failed: {err}"));
                    return;
                }
            }
        }
        self.set_active_id(entry.kind, Some(entry.id.clone()));
        self.status = format!("{} active for {}", entry.name, entry.kind.label());
        self.flash(format!(
            "{} is now the {} provider",
            entry.name,
            entry.kind.label()
        ));
        self.mark_dirty();
    }

    fn install_selected_plugin(&mut self) {
        let Some(entry) = self.plugin_entries().get(self.plugin_selected).cloned() else {
            return;
        };
        if entry.installed {
            self.set_active_id(entry.kind, Some(entry.id.clone()));
            self.flash(format!("{} is already installed and active", entry.name));
            self.mark_dirty();
            return;
        }
        match self.plugins.install(&entry.id) {
            Ok(()) => {
                self.set_active_id(entry.kind, Some(entry.id.clone()));
                self.flash(format!("installed and activated {}", entry.name));
                self.mark_dirty();
            }
            Err(err) => self.flash(format!("install failed: {err}")),
        }
    }

    fn uninstall_selected_plugin(&mut self) {
        let Some(entry) = self.plugin_entries().get(self.plugin_selected).cloned() else {
            return;
        };
        if !entry.installed {
            self.flash("plugin is not installed");
            return;
        }
        if !entry.bundled {
            self.flash("user-installed plugins must be removed from the plugins folder");
            return;
        }
        match self.plugins.uninstall(&entry.id) {
            Ok(()) => {
                if self.active_id(entry.kind).as_deref() == Some(entry.id.as_str()) {
                    let fallback = self
                        .plugins
                        .first_of_kind(entry.kind)
                        .map(|plugin| plugin.manifest.id);
                    self.set_active_id(entry.kind, fallback);
                }
                self.flash(format!("uninstalled {}", entry.name));
                self.mark_dirty();
            }
            Err(err) => self.flash(format!("uninstall failed: {err}")),
        }
    }

    fn refresh_current(&mut self) {
        match self.view {
            View::Search => {
                if !self.search_query.trim().is_empty() {
                    self.search_dirty = true;
                    self.start_search(true);
                } else {
                    self.flash("type a search first");
                }
            }
            View::Plugins => {
                self.plugins.refresh();
                self.flash("plugin registry refreshed");
            }
            View::Library => self.start_library_scan(),
            View::Browse => {
                if let Some(page) = self.browse_stack.last() {
                    let (command, argument) = if page.kind == BrowseKind::Artist {
                        ("artist", page.title.clone())
                    } else {
                        let id = page
                            .albums
                            .first()
                            .map(|album| album.id.clone())
                            .unwrap_or_default();
                        ("album", id)
                    };
                    let generation = page.generation;
                    self.start_browse_request(generation, command, argument);
                }
            }
            _ => self.flash("refreshed"),
        }
    }

    fn clamp_selections(&mut self) {
        self.search_selected = self
            .search_selected
            .min(self.search_row_count().saturating_sub(1));
        if let Some(page) = self.browse_stack.last_mut() {
            page.selected = page
                .selected
                .min(browse_selectable(page).len().saturating_sub(1));
        }
        self.queue_selected = self.queue_selected.min(self.queue.len().saturating_sub(1));
        self.favorites_selected = self
            .favorites_selected
            .min(self.favorites.len().saturating_sub(1));
        self.playlist_selected = self
            .playlist_selected
            .min(self.playlists.len().saturating_sub(1));
        if let Some(index) = self.open_playlist {
            let len = self
                .playlists
                .get(index)
                .map(|playlist| playlist.tracks.len())
                .unwrap_or(0);
            self.playlist_track_selected = self.playlist_track_selected.min(len.saturating_sub(1));
        }
        self.downloads_selected = self
            .downloads_selected
            .min(self.download_tracks().len().saturating_sub(1));
        self.history_selected = self
            .history_selected
            .min(self.history.len().saturating_sub(1));
        self.plugin_selected = self
            .plugin_selected
            .min(self.plugin_entries().len().saturating_sub(1));
        if let Some(index) = self.now_index {
            if index >= self.queue.len() {
                self.now_index = None;
            }
        }
    }

    pub fn recent_queries(&self) -> &[String] {
        &self.recent_queries
    }

    pub fn audio_snapshot(&self) -> AudioSnapshot {
        self.audio
            .as_ref()
            .map(AudioEngine::snapshot)
            .unwrap_or_default()
    }

    pub fn spinner_char(&self) -> &'static str {
        const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        FRAMES[self.spinner % FRAMES.len()]
    }

    pub fn active_plugin_name(&self, kind: PluginKind) -> String {
        self.plugins
            .active(kind, self.active_id(kind).as_deref())
            .map(|plugin| plugin.manifest.name)
            .unwrap_or_else(|| "none".to_string())
    }

    pub fn current_queue_duration(&self) -> u64 {
        self.queue.iter().filter_map(|track| track.duration).sum()
    }

    pub fn flash(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.status = message.clone();
        self.toast = Some((message, Instant::now()));
    }

    fn mark_dirty(&mut self) {
        if self.dirty_since.is_none() {
            self.dirty_since = Some(Instant::now());
        }
    }

    /// Start a search from the command line (`mfe-player daft punk`).
    pub fn prime_search(&mut self, query: String) {
        let query = query.trim().to_string();
        if query.is_empty() {
            return;
        }
        self.view = View::Search;
        self.search_query = query;
        self.search_focused = false;
        self.start_search(true);
    }

    pub fn persist(&self) {
        let state = PersistedState {
            playlists: self.playlists.clone(),
            favorites: self.favorites.clone(),
            queue: self.queue.clone(),
            now_playing: self.now_playing.clone(),
            now_index: self.now_index,
            position: self.audio_snapshot().position,
            volume: self.volume,
            speed: self.speed,
            shuffle: self.shuffle,
            repeat: match self.repeat {
                RepeatMode::Off => "off".to_string(),
                RepeatMode::All => "all".to_string(),
                RepeatMode::One => "one".to_string(),
            },
            active_metadata: self.active_metadata.clone(),
            active_streaming: self.active_streaming.clone(),
            active_lyrics: self.active_lyrics.clone(),
            recent_queries: self.recent_queries.clone(),
            radio: self.radio,
            eq: self.eq.clone(),
            normalize: self.normalize,
            history: self.history.clone(),
        };
        let _ = state.save();
    }

    pub fn background_hint(&self) -> String {
        let socket = self
            .audio
            .as_ref()
            .map(|audio| audio.socket_path().display().to_string())
            .unwrap_or_else(|| config::mpv_socket_path().display().to_string());
        format!(
            "Playing in the background — queue, repeat, shuffle and radio keep running.\n\
             Run `mfe-player` again to reattach.\n\
             Pause/resume with: mfe-player toggle\n\
             Stop it with: mfe-player stop\n\
             (mpv IPC socket: {socket})"
        )
    }
}

fn pick_active(
    plugins: &PluginRegistry,
    kind: PluginKind,
    configured: Option<&str>,
) -> Option<String> {
    if let Some(id) = configured {
        if let Some(plugin) = plugins.by_id(id) {
            if plugin.manifest.kind == kind {
                return Some(id.to_string());
            }
        }
    }
    plugins.first_of_kind(kind).map(|plugin| plugin.manifest.id)
}

fn format_clock(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

fn step_index(current: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    if delta < 0 {
        current.saturating_sub(delta.unsigned_abs()).min(len - 1)
    } else {
        (current + delta as usize).min(len - 1)
    }
}

fn unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn normalize_results(results: &mut SearchResults) {
    let mut seen_tracks = HashSet::new();
    results
        .tracks
        .retain(|track| track.is_valid() && seen_tracks.insert(track.id.clone()));

    let mut seen_artists = HashSet::new();
    results
        .artists
        .retain(|artist| artist.is_valid() && seen_artists.insert(artist.name.to_lowercase()));

    let mut seen_albums = HashSet::new();
    results.albums.retain(|album| {
        album.is_valid()
            && seen_albums.insert((album.title.to_lowercase(), album.artist.to_lowercase()))
    });

    if results.artists.is_empty() {
        let mut seen = HashSet::new();
        for track in &results.tracks {
            let key = track.artist.to_lowercase();
            if key.is_empty() || !seen.insert(key) {
                continue;
            }
            results.artists.push(Artist {
                id: format!("artist:{}", track.artist),
                name: track.artist.clone(),
                artwork: track.artwork.clone(),
                source: track.source.clone(),
                album_count: None,
            });
        }
    }

    if results.albums.is_empty() {
        let mut seen = HashSet::new();
        for track in &results.tracks {
            if track.album.trim().is_empty() {
                continue;
            }
            let key = (track.album.to_lowercase(), track.artist.to_lowercase());
            if !seen.insert(key) {
                continue;
            }
            results.albums.push(Album {
                id: format!("album:{}:{}", track.artist, track.album),
                title: track.album.clone(),
                artist: track.artist.clone(),
                artwork: track.artwork.clone(),
                year: track.year.clone(),
                track_count: None,
                source: track.source.clone(),
                tracks: Vec::new(),
            });
        }
    }

    // Attach the songs we already have so albums can be queued/added without
    // another round-trip to the metadata provider.
    for album in &mut results.albums {
        if !album.tracks.is_empty() {
            continue;
        }
        album.tracks = results
            .tracks
            .iter()
            .filter(|track| {
                track.album.eq_ignore_ascii_case(&album.title)
                    && track.artist.eq_ignore_ascii_case(&album.artist)
            })
            .cloned()
            .collect();
        if album.track_count.is_none() && !album.tracks.is_empty() {
            album.track_count = Some(album.tracks.len() as u64);
        }
    }
}

pub fn search_selectable(results: &SearchResults) -> Vec<SearchRow> {
    let mut rows = Vec::new();
    rows.extend((0..results.artists.len()).map(SearchRow::Artist));
    rows.extend((0..results.albums.len()).map(SearchRow::Album));
    rows.extend((0..results.tracks.len()).map(SearchRow::Track));
    rows
}

pub fn search_display(results: &SearchResults) -> Vec<SearchRow> {
    let mut rows = Vec::new();
    if !results.artists.is_empty() {
        rows.push(SearchRow::Header("artists"));
        rows.extend((0..results.artists.len()).map(SearchRow::Artist));
    }
    if !results.albums.is_empty() {
        rows.push(SearchRow::Header("albums"));
        rows.extend((0..results.albums.len()).map(SearchRow::Album));
    }
    if !results.tracks.is_empty() {
        rows.push(SearchRow::Header("songs"));
        rows.extend((0..results.tracks.len()).map(SearchRow::Track));
    }
    rows
}

pub fn browse_selectable(page: &BrowsePage) -> Vec<BrowseRow> {
    let mut rows = Vec::new();
    if page.kind == BrowseKind::Artist {
        rows.extend((0..page.albums.len()).map(BrowseRow::Album));
    }
    rows.extend((0..page.tracks.len()).map(BrowseRow::Track));
    rows
}

pub fn browse_display(page: &BrowsePage) -> Vec<BrowseRow> {
    let mut rows = Vec::new();
    if page.kind == BrowseKind::Artist && !page.albums.is_empty() {
        rows.push(BrowseRow::Header("albums"));
        rows.extend((0..page.albums.len()).map(BrowseRow::Album));
    }
    if !page.tracks.is_empty() {
        rows.push(BrowseRow::Header(if page.kind == BrowseKind::Artist {
            "popular tracks"
        } else {
            "tracks"
        }));
        rows.extend((0..page.tracks.len()).map(BrowseRow::Track));
    }
    rows
}

fn pseudo_random_except(len: usize, except: usize) -> usize {
    if len <= 1 {
        return 0;
    }
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos() as usize)
        .unwrap_or(0);
    let mut index = seed % len;
    if index == except {
        index = (index + 1) % len;
    }
    index
}

pub fn fuzzy_rank(query: &str, items: &[String]) -> Vec<usize> {
    if query.trim().is_empty() {
        return (0..items.len()).collect();
    }
    let matcher = SkimMatcherV2::default();
    let mut scored: Vec<(i64, usize)> = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| matcher.fuzzy_match(item, query).map(|score| (score, index)))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, index)| index).collect()
}

pub fn palette_ranked(query: &str) -> Vec<usize> {
    let labels: Vec<String> = PALETTE_ITEMS
        .iter()
        .map(|item| format!("{} {}", item.label, item.hint))
        .collect();
    fuzzy_rank(query, &labels)
}

fn unique_playlist_name(base: &str, playlists: &[Playlist]) -> String {
    let base = base.trim();
    let base = if base.is_empty() {
        "Imported link"
    } else {
        base
    };
    let mut name = base.to_string();
    let mut suffix = 2;
    while playlists
        .iter()
        .any(|playlist| playlist.name.eq_ignore_ascii_case(&name))
    {
        name = format!("{base} ({suffix})");
        suffix += 1;
    }
    name
}

fn clipboard_text() -> Option<String> {
    let attempts: [(&str, &[&str]); 3] = [
        ("wl-paste", &["--no-newline"]),
        ("xclip", &["-selection", "clipboard", "-o"]),
        ("xsel", &["-b", "-o"]),
    ];
    for (command, args) in attempts {
        let Ok(output) = std::process::Command::new(command).args(args).output() else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}

/// Import one song or a whole playlist from a URL. yt-dlp handles YouTube,
/// SoundCloud and most other supported sites; plain `.m3u` files are fetched
/// with curl and parsed locally so remote playlist files work too.
fn import_link(url: &str) -> std::result::Result<LinkImport, String> {
    let clean = url.split('?').next().unwrap_or(url).to_ascii_lowercase();
    if clean.ends_with(".m3u") {
        return import_remote_m3u(url);
    }

    let output = std::process::Command::new("yt-dlp")
        .args([
            "--flat-playlist",
            "--dump-single-json",
            "--no-warnings",
            "--no-progress",
            "--ignore-errors",
            "--playlist-end",
            "250",
            url,
        ])
        .output()
        .map_err(|err| format!("could not run yt-dlp: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail = stderr
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("yt-dlp could not read this link");
        return Err(truncate(tail, 160));
    }

    let value: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|err| format!("could not parse link metadata: {err}"))?;
    let title = value
        .get("title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Imported link")
        .trim()
        .to_string();
    let entries: Vec<serde_json::Value> =
        match value.get("entries").and_then(serde_json::Value::as_array) {
            Some(entries) => entries
                .iter()
                .filter(|entry| !entry.is_null())
                .cloned()
                .collect(),
            None => vec![value.clone()],
        };

    let mut tracks = Vec::new();
    let mut seen = HashSet::new();
    for entry in entries {
        let mut target = entry
            .get("webpage_url")
            .and_then(serde_json::Value::as_str)
            .or_else(|| entry.get("url").and_then(serde_json::Value::as_str))
            .unwrap_or_default()
            .to_string();
        if !target.starts_with("http") {
            let id = entry
                .get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let extractor = entry
                .get("ie_key")
                .or_else(|| entry.get("extractor_key"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase();
            if id.is_empty() {
                continue;
            }
            if extractor.contains("youtube") {
                target = format!("https://www.youtube.com/watch?v={id}");
            } else {
                continue;
            }
        }
        let id = format!("url:{target}");
        if !seen.insert(id.clone()) {
            continue;
        }
        let raw_title = entry
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        let track_title = if raw_title.is_empty() {
            target.clone()
        } else {
            raw_title
        };
        let artist = entry
            .get("artist")
            .or_else(|| entry.get("creator"))
            .or_else(|| entry.get("uploader"))
            .or_else(|| entry.get("channel"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        let album = entry
            .get("album")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        let duration = entry
            .get("duration")
            .and_then(serde_json::Value::as_f64)
            .map(|seconds| seconds.max(0.0).round() as u64);
        let artwork = entry
            .get("thumbnail")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                entry
                    .get("thumbnails")
                    .and_then(serde_json::Value::as_array)
                    .and_then(|items| items.last())
                    .and_then(|item| item.get("url"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            });
        tracks.push(Track {
            id,
            title: track_title,
            artist,
            album,
            duration,
            artwork,
            source: "link".to_string(),
            year: None,
        });
    }

    if tracks.is_empty() {
        return Err("the link contained no playable tracks".to_string());
    }
    Ok(LinkImport { title, tracks })
}

fn import_remote_m3u(url: &str) -> std::result::Result<LinkImport, String> {
    let output = std::process::Command::new("curl")
        .args(["-fsSL", "--max-time", "30", url])
        .output()
        .map_err(|err| format!("could not run curl: {err}"))?;
    if !output.status.success() {
        return Err("could not download the playlist file".to_string());
    }
    let raw = String::from_utf8_lossy(&output.stdout);
    let mut tracks: Vec<Track> = Vec::new();
    let mut pending: Option<(String, String)> = None;
    for line in raw.lines().map(str::trim) {
        if line.is_empty() || line == "#EXTM3U" {
            continue;
        }
        if let Some(meta) = line.strip_prefix("#EXTINF:") {
            if let Some((_, label)) = meta.split_once(',') {
                let (artist, title) = match label.split_once(" - ") {
                    Some((artist, title)) => (artist.trim().to_string(), title.trim().to_string()),
                    None => (String::new(), label.trim().to_string()),
                };
                pending = Some((artist, title));
            }
        } else if let Some(json) = line
            .strip_prefix("#MFE:")
            .or_else(|| line.strip_prefix("#NUCLEAR:"))
        {
            if let Ok(track) = serde_json::from_str::<Track>(json) {
                let replace_placeholder = tracks.last().is_some_and(|last| {
                    last.id.starts_with("imported:")
                        && (last.title == track.title
                            || last.title.trim_end_matches(['-', ' ']).trim()
                                == track.artist.trim())
                });
                if replace_placeholder {
                    if let Some(last) = tracks.last_mut() {
                        *last = track;
                    }
                } else {
                    tracks.push(track);
                }
                pending = None;
            }
        } else if !line.starts_with('#') {
            if let Some((artist, title)) = pending.take() {
                if line.starts_with("http") {
                    tracks.push(Track {
                        id: format!("url:{line}"),
                        title: if title.is_empty() {
                            line.to_string()
                        } else {
                            title
                        },
                        artist,
                        source: "link".to_string(),
                        ..Default::default()
                    });
                }
            }
        }
    }
    let name = url
        .rsplit('/')
        .next()
        .unwrap_or("Imported playlist")
        .trim_end_matches(".m3u")
        .to_string();
    if tracks.is_empty() {
        return Err("the playlist file had nohttp tracks".to_string());
    }
    Ok(LinkImport {
        title: if name.is_empty() {
            "Imported playlist".to_string()
        } else {
            name
        },
        tracks,
    })
}

fn load_downloads() -> HashMap<String, (Track, PathBuf)> {
    let dir = config::downloads_dir();
    let mut downloads = HashMap::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return downloads;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(track) = serde_json::from_str::<Track>(&raw) else {
            continue;
        };
        let stem = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let audio = std::fs::read_dir(&dir).ok().and_then(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .find(|candidate| {
                    let matches_stem = candidate
                        .file_stem()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name == stem);
                    let is_audio = candidate
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .is_some_and(|ext| ext != "json" && ext != "part");
                    matches_stem && is_audio
                })
        });
        if let Some(audio) = audio {
            downloads.insert(track.id.clone(), (track, audio));
        }
    }
    downloads
}

fn safe_filename(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        if ch.is_alphanumeric() || ch == '-' || ch == '_' || ch == ' ' {
            out.push(ch);
        } else {
            out.push('_');
        }
        if out.len() >= 80 {
            break;
        }
    }
    out.trim().replace(' ', "_")
}

fn rect_contains(rect: Rect, column: u16, row: u16) -> bool {
    column >= rect.x
        && column < rect.x.saturating_add(rect.width)
        && row >= rect.y
        && row < rect.y.saturating_add(rect.height)
}
