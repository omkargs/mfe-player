use std::fs;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config;
use crate::models::{Playlist, Track};

fn default_volume() -> f64 {
    70.0
}

fn default_repeat() -> String {
    "off".to_string()
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
    pub shuffle: bool,
    pub repeat: String,
    pub active_metadata: Option<String>,
    pub active_streaming: Option<String>,
    pub active_lyrics: Option<String>,
    pub recent_queries: Vec<String>,
    pub radio: bool,
    pub eq: String,
    pub normalize: bool,
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
            shuffle: false,
            repeat: default_repeat(),
            active_metadata: None,
            active_streaming: None,
            active_lyrics: None,
            recent_queries: Vec::new(),
            radio: false,
            eq: "flat".to_string(),
            normalize: false,
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
        fs::rename(&tmp, &path).with_context(|| format!("replacing {}", path.display()))?;
        Ok(())
    }
}
