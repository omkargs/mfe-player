use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: Option<u64>,
    pub artwork: Option<String>,
    pub source: String,
    pub year: Option<String>,
    /// Explicit genres from metadata plugins (iTunes, MusicBrainz, radio).
    /// Weighted higher than free-form tags by the taste engine.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub genre: Vec<String>,
    /// Free-form tags from plugins or local files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Set when autoplay queued the track instead of the listener.
    #[serde(default, skip_serializing_if = "is_false")]
    pub auto_queued: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl Track {
    pub fn search_text(&self) -> String {
        format!(
            "{} {} {} {}",
            self.title, self.artist, self.album, self.source
        )
    }

    pub fn duration_label(&self) -> String {
        self.duration
            .map(format_duration)
            .unwrap_or_else(|| "--:--".to_string())
    }

    pub fn is_valid(&self) -> bool {
        !self.id.trim().is_empty() && !self.title.trim().is_empty()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub tracks: Vec<Track>,
}

impl Playlist {
    pub fn duration(&self) -> u64 {
        self.tracks.iter().filter_map(|t| t.duration).sum()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Artist {
    pub id: String,
    pub name: String,
    pub artwork: Option<String>,
    pub source: String,
    pub album_count: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Album {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub artwork: Option<String>,
    pub year: Option<String>,
    pub track_count: Option<u64>,
    pub source: String,
    #[serde(default)]
    pub tracks: Vec<Track>,
}

impl Album {
    pub fn is_valid(&self) -> bool {
        !self.title.trim().is_empty() && (!self.id.trim().is_empty() || !self.tracks.is_empty())
    }
}

impl Artist {
    pub fn is_valid(&self) -> bool {
        !self.name.trim().is_empty()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchResults {
    pub artists: Vec<Artist>,
    pub albums: Vec<Album>,
    pub tracks: Vec<Track>,
}

impl SearchResults {
    pub fn len(&self) -> usize {
        self.artists.len() + self.albums.len() + self.tracks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SearchPayload {
    Full(SearchResults),
    Tracks(Vec<Track>),
}

impl SearchPayload {
    pub fn into_results(self) -> SearchResults {
        match self {
            Self::Full(results) => results,
            Self::Tracks(tracks) => SearchResults {
                artists: Vec::new(),
                albums: Vec::new(),
                tracks,
            },
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StreamInfo {
    pub url: String,
    #[serde(default)]
    pub backup_url: Option<String>,
    #[serde(default)]
    pub webpage_url: Option<String>,
    pub title: Option<String>,
    pub duration: Option<u64>,
    pub extractor: Option<String>,
    #[serde(default)]
    pub http_headers: HashMap<String, String>,
    /// Set by streaming providers that could only offer a short ad-style
    /// preview clip instead of the full track. The player treats that as a
    /// failure and falls through to the next provider.
    #[serde(default)]
    pub short_preview: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LyricsDoc {
    pub source: String,
    pub plain: Option<String>,
    pub synced: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LyricLine {
    pub at: Option<f64>,
    pub text: String,
}

pub fn parse_lyrics(doc: &LyricsDoc) -> Vec<LyricLine> {
    if let Some(synced) = doc.synced.as_deref().filter(|s| !s.trim().is_empty()) {
        let lines = parse_lrc(synced);
        if !lines.is_empty() {
            return lines;
        }
    }
    doc.plain
        .as_deref()
        .unwrap_or("")
        .lines()
        .map(|line| LyricLine {
            at: None,
            text: line.trim_end().to_string(),
        })
        .collect()
}

fn parse_lrc(input: &str) -> Vec<LyricLine> {
    let mut out = Vec::new();
    for raw in input.lines() {
        let mut rest = raw.trim();
        let mut stamps = Vec::new();
        while let Some(after_open) = rest.strip_prefix('[') {
            let Some(close) = after_open.find(']') else {
                break;
            };
            let stamp = &after_open[..close];
            if let Some(seconds) = parse_timestamp(stamp) {
                stamps.push(seconds);
                rest = after_open[close + 1..].trim_start();
            } else {
                break;
            }
        }
        if stamps.is_empty() {
            let trimmed = rest.trim();
            let metadata_tag = trimmed
                .strip_prefix('[')
                .and_then(|after| after.split_once(']'))
                .map(|(tag, _)| {
                    matches!(
                        tag.split(':')
                            .next()
                            .unwrap_or("")
                            .trim()
                            .to_ascii_lowercase()
                            .as_str(),
                        "ar" | "ti" | "al" | "by" | "offset" | "re" | "ve" | "length"
                    )
                })
                .unwrap_or(false);
            if !trimmed.is_empty() && !metadata_tag {
                out.push(LyricLine {
                    at: None,
                    text: rest.to_string(),
                });
            }
            continue;
        }
        for at in stamps {
            out.push(LyricLine {
                at: Some(at),
                text: rest.to_string(),
            });
        }
    }
    out.sort_by(|a, b| match (a.at, b.at) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        _ => std::cmp::Ordering::Equal,
    });
    out
}

fn parse_timestamp(stamp: &str) -> Option<f64> {
    let mut parts = stamp.split(':');
    let minutes: f64 = parts.next()?.parse().ok()?;
    let seconds: f64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(minutes * 60.0 + seconds)
}

pub fn format_duration(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}")
    }
}

pub fn format_position(seconds: f64) -> String {
    let seconds = seconds.max(0.0).round() as u64;
    format_duration(seconds)
}

pub fn truncate(text: &str, max_width: usize) -> String {
    use unicode_width::UnicodeWidthStr;
    if max_width == 0 {
        return String::new();
    }
    if text.width() <= max_width {
        return text.to_string();
    }
    if max_width == 1 {
        return "…".to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > max_width - 1 {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

pub fn display_width(text: &str) -> usize {
    use unicode_width::UnicodeWidthStr;
    text.width()
}
