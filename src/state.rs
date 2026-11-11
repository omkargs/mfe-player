use std::collections::HashMap;
use std::fs;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config;
use crate::models::{Playlist, Track};

fn default_volume() -> f64 {
    70.0
}

fn default_speed() -> f64 {
    1.0
}

fn default_repeat() -> String {
    "off".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub track: Track,
    /// Unix timestamp (seconds) when playback started.
    pub played_at: u64,
}

fn default_history() -> Vec<HistoryEntry> {
    Vec::new()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PersistedState {
    pub playlists: Vec<Playlist>,
    pub favorites: Vec<Track>,
    pub queue: Vec<Track>,
    pub now_playing: Option<Track>,
    pub now_index: Option<usize>,
    pub position: f64,
    pub volume: f64,
    pub speed: f64,
    pub shuffle: bool,
    pub repeat: String,
    pub active_metadata: Option<String>,
    pub active_streaming: Option<String>,
    pub active_lyrics: Option<String>,
    pub recent_queries: Vec<String>,
    pub radio: bool,
    pub eq: String,
    pub normalize: bool,
    #[serde(default = "default_history")]
    pub history: Vec<HistoryEntry>,
    /// Custom ten-band EQ curve in dB.
    #[serde(default)]
    pub eq_gains: Vec<f64>,
    /// Last known position per track id, for long mixes and podcasts.
    #[serde(default)]
    pub bookmarks: HashMap<String, f64>,
}

impl Default for PersistedState {
    fn default() -> Self {
        Self {
            playlists: Vec::new(),
            favorites: Vec::new(),
            queue: Vec::new(),
            now_playing: None,
            now_index: None,
            position: 0.0,
            volume: default_volume(),
            speed: default_speed(),
            shuffle: false,
            repeat: default_repeat(),
            active_metadata: None,
            active_streaming: None,
            active_lyrics: None,
            recent_queries: Vec::new(),
            radio: false,
            eq: "flat".to_string(),
            normalize: false,
            history: Vec::new(),
            eq_gains: Vec::new(),
            bookmarks: HashMap::new(),
        }
    }
}

impl PersistedState {
    pub fn load() -> (Self, Option<String>) {
        let path = config::state_path();
        if !path.exists() {
            return (Self::default(), None);
        }
        match fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<Self>(&raw) {
                Ok(mut state) => {
                    state.volume = state.volume.clamp(0.0, 130.0);
                    if !(0.25..=4.0).contains(&state.speed) {
                        state.speed = 1.0;
                    }
                    (state, None)
                }
                Err(err) => (
                    Self::default(),
                    Some(format!("state.json is invalid, using defaults: {err}")),
                ),
            },
            Err(err) => (
                Self::default(),
                Some(format!("could not read state.json: {err}")),
            ),
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = config::state_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        let raw = serde_json::to_string_pretty(self)?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, raw).with_context(|| format!("writing {}", tmp.display()))?;
        crate::platform::fs_util::replace_file(&tmp, &path)
            .with_context(|| format!("replacing {}", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_state_files_get_empty_bookmarks() {
        let state: PersistedState = serde_json::from_str("{}").unwrap();
        assert!(state.bookmarks.is_empty());
    }

    #[test]
    fn bookmarks_survive_a_round_trip() {
        let mut state = PersistedState::default();
        state.bookmarks.insert("track:1".to_string(), 123.5);
        let json = serde_json::to_string(&state).unwrap();
        let back: PersistedState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.bookmarks.get("track:1"), Some(&123.5));
    }
}
