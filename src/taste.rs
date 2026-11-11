//! Local taste profile and autoplay scoring.
//!
//! The player used to treat "radio" as one metadata search when the queue ran
//! out. This module keeps a small, explainable model of what the listener
//! actually finishes, repeats, likes and skips, and uses it to rank candidate
//! tracks for autoplay.
//!
//! Two profiles are kept:
//!
//! * **long term** — decays with a 30-day half-life and drives the general
//!   taste (genres, artists, sources).
//! * **session** — decays with a 90-minute half-life and resets after 30
//!   minutes of silence. Skips only touch this profile, so one bad evening does
//!   not permanently ruin a genre, but five skipped phonk tracks do change what
//!   is suggested for the rest of the session.
//!
//! Everything is stored in `~/.config/mfe-player/taste.json`. No network, no
//! account, and every recommendation can be explained with `mfe taste explain`.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config;
use crate::models::Track;

const LONG_TERM_HALF_LIFE_DAYS: f64 = 30.0;
const SESSION_HALF_LIFE_MINUTES: f64 = 90.0;
const SESSION_IDLE_RESET_SECS: u64 = 30 * 60;
const EVENT_CAP: usize = 2000;
const AUTO_POSITIVE_WEIGHT: f64 = 0.6;
const BLOCK_SECONDS: u64 = 30 * 24 * 60 * 60;
const PROBE_COOLDOWN_SECONDS: u64 = 7 * 24 * 60 * 60;

/// How autoplay picks the next batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AutoplayMode {
    /// Stop when the queue ends.
    Off,
    /// Keep the old behaviour: search around the current artist/title.
    Similar,
    /// Rank candidates with the taste profile.
    #[default]
    Taste,
}

impl AutoplayMode {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "off" | "false" | "no" => Self::Off,
            "similar" | "radio" => Self::Similar,
            _ => Self::Taste,
        }
    }

}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Complete,
    Half,
    SoftSkip,
    Skip,
    QuickSkip,
    Like,
    Repeat,
    Dislike,
    Error,
}

impl EventKind {
    fn weight(self) -> f64 {
        match self {
            Self::Like => 3.0,
            Self::Repeat => 2.0,
            Self::Complete => 1.0,
            Self::Half => 0.5,
            Self::SoftSkip => -0.3,
            Self::Skip => -1.0,
            Self::QuickSkip => -2.0,
            Self::Dislike => -4.0,
            Self::Error => 0.0,
        }
    }

    fn positive(self) -> bool {
        self.weight() > 0.0
    }

    fn negative(self) -> bool {
        self.weight() < 0.0
    }

    fn skipped(self) -> bool {
        matches!(self, Self::SoftSkip | Self::Skip | Self::QuickSkip)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TasteEvent {
    pub id: String,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub outcome: EventKind,
    #[serde(default)]
    pub fraction: f64,
    #[serde(default)]
    pub auto: bool,
    #[serde(default)]
    pub probe: bool,
    pub at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Affinity {
    #[serde(default)]
    pub tags: HashMap<String, f64>,
    #[serde(default)]
    pub artists: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub started_at: u64,
    #[serde(default)]
    pub last_at: u64,
    #[serde(default)]
    pub tags: HashMap<String, f64>,
    #[serde(default)]
    pub artists: HashMap<String, f64>,
    #[serde(default)]
    pub plays: u32,
    #[serde(default)]
    pub skips: u32,
    #[serde(default)]
    pub completed: u32,
    #[serde(default)]
    pub autoplay_picks: u32,
    #[serde(default)]
    pub since_probe: u32,
}

impl Default for Session {
    fn default() -> Self {
        let now = now_unix();
        Self {
            id: format!("s-{now}"),
            started_at: now,
            last_at: now,
            tags: HashMap::new(),
            artists: HashMap::new(),
            plays: 0,
            skips: 0,
            completed: 0,
            autoplay_picks: 0,
            since_probe: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Probes {
    #[serde(default)]
    pub trials: u32,
    #[serde(default)]
    pub successes: u32,
    #[serde(default)]
    pub failures: u32,
    #[serde(default)]
    pub failures_by_tag: HashMap<String, u32>,
    #[serde(default)]
    pub cooldown_until: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LastPick {
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub tag_score: f64,
    #[serde(default)]
    pub artist_score: f64,
    #[serde(default)]
    pub session_score: f64,
    #[serde(default)]
    pub novelty: f64,
    #[serde(default)]
    pub total: f64,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub probe: bool,
    #[serde(default)]
    pub at: u64,
}

#[derive(Debug, Clone)]
struct Observation {
    track_id: String,
    title: String,
    artist: String,
    tags: HashMap<String, f64>,
    position: f64,
    duration: f64,
    auto: bool,
    probe: bool,
    probe_tag: Option<String>,
}

/// Serialized taste profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Taste {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub updated_at: u64,
    #[serde(default)]
    pub long_term: Affinity,
    #[serde(default)]
    pub session: Session,
    #[serde(default)]
    pub events: VecDeque<TasteEvent>,
    #[serde(default)]
    pub probes: Probes,
    /// Track id → unix time until which it should not be suggested.
    #[serde(default)]
    pub blocked: HashMap<String, u64>,
    /// Tags that came from genre metadata or plugin tags (not title words).
    /// Only these are offered as mismatch probes.
    #[serde(default)]
    pub known_tags: HashSet<String>,
    #[serde(default)]
    pub last_pick: Option<LastPick>,
    /// True once history has been used to warm the profile. A manual reset
    /// clears the profile but keeps this true, so we do not silently rebuild it.
    #[serde(default)]
    pub seeded: bool,
    #[serde(skip)]
    current: Option<Observation>,
    #[serde(skip)]
    dirty: bool,
    #[serde(skip)]
    last_save: Option<Instant>,
}

fn default_version() -> u32 {
    1
}

impl Default for Taste {
    fn default() -> Self {
        Self {
            version: 1,
            updated_at: now_unix(),
            long_term: Affinity::default(),
            session: Session::default(),
            events: VecDeque::new(),
            probes: Probes::default(),
            blocked: HashMap::new(),
            known_tags: HashSet::new(),
            last_pick: None,
            seeded: false,
            current: None,
            dirty: false,
            last_save: None,
        }
    }
}

/// Result of planning an autoplay batch.
#[derive(Debug, Clone, Default)]
pub struct BatchPlan {
    pub tracks: Vec<Track>,
    /// track id → adjacent tag that this track is probing.
    pub probes: HashMap<String, String>,
}

#[derive(Debug, Clone)]
struct ScoreParts {
    tag: f64,
    artist: f64,
    session: f64,
    novelty: f64,
    total: f64,
    primary_tag: String,
    reason: String,
}

#[derive(Debug, Clone)]
struct Candidate {
    track: Track,
    tags: HashMap<String, f64>,
    parts: ScoreParts,
    artist_key: String,
}

impl Taste {
    pub fn load() -> (Self, Option<String>) {
        let path = config::taste_path();
        let mut taste = match fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<Self>(&raw) {
                Ok(taste) => taste,
                Err(err) => (
                    Self::default(),
                    Some(format!("taste.json is invalid, starting fresh: {err}")),
                )
                    .0,
            },
            Err(_) => Self::default(),
        };
        let now = now_unix();
        taste.decay(now);
        taste.last_save = Some(Instant::now());
        let warning = if path.exists() {
            None
        } else {
            None
        };
        (taste, warning)
    }

    pub fn save(&self) -> Result<()> {
        let path = config::taste_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let raw = serde_json::to_string_pretty(self)?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, raw).with_context(|| format!("writing {}", tmp.display()))?;
        fs::rename(&tmp, &path).with_context(|| format!("replacing {}", path.display()))?;
        Ok(())
    }

    /// Save at most every 15 seconds; called from the app tick.
    pub fn flush_if_due(&mut self) {
        if !self.dirty {
            return;
        }
        let due = self
            .last_save
            .is_none_or(|at| at.elapsed() >= Duration::from_secs(15));
        if due {
            let _ = self.save();
            self.dirty = false;
            self.last_save = Some(Instant::now());
        }
    }

    /// Force a write (quit, daemon shutdown).
    pub fn flush(&mut self) {
        if self.dirty {
            let _ = self.save();
            self.dirty = false;
            self.last_save = Some(Instant::now());
        }
    }

    /// Warm the long-term profile from existing history/favourites the first
    /// time taste is enabled. Played tracks count less than explicit favourites.
    pub fn seed_if_empty(&mut self, history: &[Track], favorites: &[Track]) {
        if self.seeded
            || !self.long_term.tags.is_empty()
            || !self.long_term.artists.is_empty()
        {
            return;
        }
        for (index, track) in history.iter().take(200).enumerate() {
            let weight = 0.25 * (1.0 - index as f64 / 400.0).max(0.25);
            self.seed_track(track, weight);
        }
        for track in favorites.iter().take(100) {
            self.seed_track(track, 1.0);
        }
        self.seeded = true;
        self.dirty = true;
    }

    fn seed_track(&mut self, track: &Track, weight: f64) {
        self.known_tags.extend(explicit_tags(track));
        let tags = tags_for(track);
        for (tag, w) in &tags {
            *self.long_term.tags.entry(tag.clone()).or_insert(0.0) += weight * w;
        }
        let artist = artist_key(&track.artist);
        if !artist.is_empty() {
            *self.long_term.artists.entry(artist).or_insert(0.0) += weight;
        }
    }

    /// Call when playback of `track` starts. Retries of the same open track are
    /// ignored so a failed resolve does not double count.
    pub fn begin(&mut self, track: &Track, auto: bool, probe_tag: Option<String>) {
        if let Some(current) = &self.current {
            if current.track_id == track.id {
                return;
            }
        }
        let now = now_unix();
        self.decay(now);
        let probe = probe_tag.is_some();
        if auto {
            self.session.autoplay_picks = self.session.autoplay_picks.saturating_add(1);
            if probe {
                self.session.since_probe = 0;
                self.probes.trials = self.probes.trials.saturating_add(1);
            } else {
                self.session.since_probe = self.session.since_probe.saturating_add(1);
            }
        }
        self.known_tags.extend(explicit_tags(track));
        self.session.plays = self.session.plays.saturating_add(1);
        self.session.last_at = now;
        self.current = Some(Observation {
            track_id: track.id.clone(),
            title: track.title.clone(),
            artist: track.artist.clone(),
            tags: tags_for(track),
            position: 0.0,
            duration: track.duration.unwrap_or(0) as f64,
            auto,
            probe,
            probe_tag,
        });
        self.dirty = true;
    }

    pub fn progress(&mut self, position: f64) {
        if let Some(current) = self.current.as_mut() {
            if position.is_finite() && position > current.position {
                current.position = position;
            }
        }
    }

    /// The track reached its natural end.
    pub fn complete(&mut self) {
        let Some(current) = self.current.as_ref() else {
            return;
        };
        let kind = if current.duration > 0.0 {
            let fraction = (current.position / current.duration).clamp(0.0, 1.0);
            if fraction >= 0.9 {
                EventKind::Complete
            } else if fraction >= 0.5 {
                EventKind::Half
            } else {
                // mpv can end a short stream before the reported duration.
                EventKind::Complete
            }
        } else {
            EventKind::Complete
        };
        self.finish(kind);
    }

    /// User pressed next/previous or picked another track.
    pub fn skip(&mut self, position: f64) {
        self.progress(position);
        let Some(current) = self.current.as_ref() else {
            return;
        };
        let duration = current.duration;
        let position = current.position;
        let fraction = if duration > 0.0 {
            (position / duration).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let kind = if position < 15.0 {
            EventKind::QuickSkip
        } else if position < 40.0 && fraction < 0.5 {
            EventKind::Skip
        } else if fraction < 0.4 {
            EventKind::Skip
        } else if fraction < 0.75 {
            EventKind::SoftSkip
        } else {
            EventKind::Half
        };
        self.finish(kind);
    }

    /// Playback failed; no taste signal either way.
    pub fn failed(&mut self) {
        self.finish(EventKind::Error);
    }

    pub fn like(&mut self, track: &Track) {
        self.signal_track(track, EventKind::Like);
    }

    pub fn dislike(&mut self, track: &Track) {
        self.signal_track(track, EventKind::Dislike);
        self.blocked
            .insert(track.id.clone(), now_unix().saturating_add(BLOCK_SECONDS));
    }

    fn signal_track(&mut self, track: &Track, kind: EventKind) {
        let now = now_unix();
        self.decay(now);
        self.known_tags.extend(explicit_tags(track));
        let tags = tags_for(track);
        let artist = artist_key(&track.artist);
        let weight = kind.weight();
        for (tag, w) in &tags {
            *self.session.tags.entry(tag.clone()).or_insert(0.0) += weight * w;
            *self.long_term.tags.entry(tag.clone()).or_insert(0.0) += weight * w;
        }
        if !artist.is_empty() {
            *self.session.artists.entry(artist.clone()).or_insert(0.0) += weight;
            *self.long_term.artists.entry(artist).or_insert(0.0) += weight;
        }
        self.session.last_at = now;
        self.updated_at = now;
        self.events.push_front(TasteEvent {
            id: track.id.clone(),
            title: track.title.clone(),
            artist: track.artist.clone(),
            tags: tags.keys().cloned().collect(),
            outcome: kind,
            fraction: 0.0,
            auto: false,
            probe: false,
            at: now,
        });
        self.trim_events();
        self.dirty = true;
    }

    fn finish(&mut self, kind: EventKind) {
        let Some(observation) = self.current.take() else {
            return;
        };
        let now = now_unix();
        self.decay(now);

        let weight = if observation.auto && kind.positive() {
            kind.weight() * AUTO_POSITIVE_WEIGHT
        } else {
            kind.weight()
        };

        // Negative signals are session-only. A skipped song should shape the
        // rest of tonight, not permanently blacklist a genre.
        let session_only = kind.negative();
        if weight != 0.0 {
            for (tag, tag_weight) in &observation.tags {
                *self.session.tags.entry(tag.clone()).or_insert(0.0) += weight * tag_weight;
                if !session_only {
                    *self.long_term.tags.entry(tag.clone()).or_insert(0.0) += weight * tag_weight;
                }
            }
            let artist = artist_key(&observation.artist);
            if !artist.is_empty() {
                *self.session.artists.entry(artist.clone()).or_insert(0.0) += weight;
                if !session_only {
                    *self.long_term.artists.entry(artist).or_insert(0.0) += weight;
                }
            }
        }

        if kind.skipped() {
            self.session.skips = self.session.skips.saturating_add(1);
        } else if kind.positive() {
            self.session.completed = self.session.completed.saturating_add(1);
        }

        if observation.probe {
            self.evaluate_probe(&observation, kind, now);
        }

        let fraction = if observation.duration > 0.0 {
            (observation.position / observation.duration).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.events.push_front(TasteEvent {
            id: observation.track_id.clone(),
            title: observation.title.clone(),
            artist: observation.artist.clone(),
            tags: observation.tags.keys().cloned().collect(),
            outcome: kind,
            fraction,
            auto: observation.auto,
            probe: observation.probe,
            at: now,
        });
        self.trim_events();

        self.session.last_at = now;
        self.updated_at = now;
        self.clamp_maps();
        self.dirty = true;
    }

    fn evaluate_probe(&mut self, observation: &Observation, kind: EventKind, now: u64) {
        let Some(tag) = observation.probe_tag.clone() else {
            return;
        };
        if kind.positive() {
            self.probes.successes = self.probes.successes.saturating_add(1);
            *self.long_term.tags.entry(tag.clone()).or_insert(0.0) += 0.3;
            self.probes.failures_by_tag.remove(&tag);
        } else if kind.negative() {
            self.probes.failures = self.probes.failures.saturating_add(1);
            *self.long_term.tags.entry(tag.clone()).or_insert(0.0) -= 0.2;
            let failures = self
                .probes
                .failures_by_tag
                .entry(tag.clone())
                .or_insert(0);
            *failures = failures.saturating_add(1);
            if *failures >= 2 {
                self.probes
                    .cooldown_until
                    .insert(tag, now.saturating_add(PROBE_COOLDOWN_SECONDS));
            }
        }
    }

    /// Rank a candidate pool with the taste model and pick `limit` tracks.
    /// Production autoplay uses `plan_batch`; this simpler entry point keeps
    /// the ranking logic directly testable.
    #[cfg(test)]
    pub fn rank(&mut self, candidates: &[Track], recent: &[Track], limit: usize) -> Vec<Track> {
        self.rank_candidates(candidates, recent, limit)
            .into_iter()
            .map(|candidate| candidate.track)
            .collect()
    }

    fn rank_candidates(
        &mut self,
        candidates: &[Track],
        recent: &[Track],
        limit: usize,
    ) -> Vec<Candidate> {
        if limit == 0 {
            return Vec::new();
        }
        let now = now_unix();
        let recent_ids: HashSet<String> = recent.iter().map(|track| track.id.clone()).collect();
        let recent_artists: HashSet<String> = recent
            .iter()
            .map(|track| artist_key(&track.artist))
            .filter(|key| !key.is_empty())
            .collect();
        let last_primary = recent
            .first()
            .and_then(|track| primary_tag(&tags_for(track)));

        let mut seen = HashSet::new();
        let mut pool = Vec::new();
        for track in candidates {
            if !track.is_valid()
                || !seen.insert(track.id.clone())
                || recent_ids.contains(&track.id)
                || self
                    .blocked
                    .get(&track.id)
                    .is_some_and(|until| *until > now)
            {
                continue;
            }
            let tags = tags_for(track);
            if tags.is_empty() {
                continue;
            }
            let parts = self.score_parts(&tags, track, &recent_artists, last_primary.as_deref());
            pool.push(Candidate {
                track: track.clone(),
                tags,
                artist_key: artist_key(&track.artist),
                parts,
            });
        }
        if pool.is_empty() {
            return Vec::new();
        }

        let mut picked: Vec<Candidate> = Vec::new();
        let mut artist_count: HashMap<String, usize> = HashMap::new();
        let mut tag_count: HashMap<String, usize> = HashMap::new();
        let tag_cap = ((limit as f64 * 0.6).ceil() as usize).max(1);

        while picked.len() < limit && !pool.is_empty() {
            let mut best = 0usize;
            let mut best_value = f64::NEG_INFINITY;
            for (index, candidate) in pool.iter().enumerate() {
                let similarity = picked
                    .iter()
                    .map(|chosen| jaccard(&chosen.tags, &candidate.tags))
                    .fold(0.0, f64::max);
                let mut value = candidate.parts.total - 0.7 * similarity;
                if candidate.artist_key.is_empty() {
                    // Keep unknown-artist tracks but do not let them dominate.
                    value -= 0.05;
                } else if artist_count
                    .get(&candidate.artist_key)
                    .copied()
                    .unwrap_or(0)
                    >= 1
                {
                    value -= 0.35;
                }
                if tag_count
                    .get(&candidate.parts.primary_tag)
                    .copied()
                    .unwrap_or(0)
                    >= tag_cap
                {
                    value -= 1.0;
                }
                if value > best_value {
                    best_value = value;
                    best = index;
                }
            }
            let candidate = pool.remove(best);
            *artist_count
                .entry(candidate.artist_key.clone())
                .or_insert(0) += 1;
            *tag_count
                .entry(candidate.parts.primary_tag.clone())
                .or_insert(0) += 1;
            picked.push(candidate);
        }

        if let Some(top) = picked.first() {
            self.last_pick = Some(LastPick {
                title: top.track.title.clone(),
                artist: top.track.artist.clone(),
                tag_score: top.parts.tag,
                artist_score: top.parts.artist,
                session_score: top.parts.session,
                novelty: top.parts.novelty,
                total: top.parts.total,
                reason: top.parts.reason.clone(),
                probe: false,
                at: now,
            });
        }
        picked
    }

    /// Rank a batch and optionally replace one early pick with a deliberate
    /// out-of-profile probe from an adjacent tag.
    pub fn plan_batch(
        &mut self,
        candidates: &[Track],
        recent: &[Track],
        limit: usize,
        probe_enabled: bool,
        probe_every: u32,
    ) -> BatchPlan {
        let mut picked = self.rank_candidates(candidates, recent, limit);
        let mut probes = HashMap::new();
        if picked.is_empty() {
            return BatchPlan::default();
        }

        let every = probe_every.max(1);
        if probe_enabled
            && self.session.plays >= 8
            && self.session.since_probe >= every
            && self.confidence() >= 0.55
        {
            if let Some(tag) = self.adjacent_tag() {
                if !self
                    .probes
                    .cooldown_until
                    .get(&tag)
                    .is_some_and(|until| *until > now_unix())
                {
                    let recent_ids: HashSet<&str> =
                        recent.iter().map(|track| track.id.as_str()).collect();
                    let chosen_ids: HashSet<String> =
                        picked.iter().map(|c| c.track.id.clone()).collect();
                    let mut best: Option<(&Track, f64)> = None;
                    for track in candidates {
                        if chosen_ids.contains(&track.id)
                            || recent_ids.contains(track.id.as_str())
                            || self
                                .blocked
                                .get(&track.id)
                                .is_some_and(|until| *until > now_unix())
                        {
                            continue;
                        }
                        let tags = tags_for(track);
                        if !tags.contains_key(&tag) {
                            continue;
                        }
                        let total = self.score_parts(&tags, track, &HashSet::new(), None).total;
                        if best.as_ref().is_none_or(|(_, seen)| total > *seen) {
                            best = Some((track, total));
                        }
                    }
                    if let Some((probe, _)) = best {
                        let insert_at = (picked.len() / 3).min(picked.len());
                        picked.insert(insert_at, self.candidate_for(probe));
                        picked.truncate(limit.max(1));
                        probes.insert(probe.id.clone(), tag);
                    }
                }
            }
        }

        let tracks = picked.into_iter().map(|candidate| candidate.track).collect();
        BatchPlan { tracks, probes }
    }

    fn candidate_for(&self, track: &Track) -> Candidate {
        let tags = tags_for(track);
        Candidate {
            track: track.clone(),
            artist_key: artist_key(&track.artist),
            parts: self.score_parts(&tags, track, &HashSet::new(), None),
            tags,
        }
    }

    fn score_parts(
        &self,
        tags: &HashMap<String, f64>,
        track: &Track,
        recent_artists: &HashSet<String>,
        last_primary: Option<&str>,
    ) -> ScoreParts {
        let tag = self.tag_similarity(tags);
        let session = self.session_similarity(tags);
        let artist = self.artist_similarity(&track.artist);
        let primary = primary_tag(tags).unwrap_or_default();
        let same_vibe_penalty = last_primary.is_some_and(|last| last == primary);
        let artist_novel = !recent_artists.contains(&artist_key(&track.artist));
        let novelty = if artist_novel {
            if same_vibe_penalty {
                0.6
            } else {
                1.0
            }
        } else {
            0.0
        };
        let explore = if self.top_effective_tags(3).iter().any(|(tag, _)| tag == &primary) {
            0.0
        } else {
            0.3
        };
        // A genre the listener has repeatedly skipped tonight must not sneak
        // back in just because MMR wants variety. Penalise in proportion to the
        // vetoed tag weight so one stray skip is a nudge, five is decisive.
        let veto: f64 = tags
            .iter()
            .filter(|(tag, _)| {
                self.session
                    .tags
                    .get(*tag)
                    .is_some_and(|weight| *weight <= -2.0)
            })
            .map(|(_, weight)| *weight)
            .sum();
        let total = 0.35 * tag + 0.25 * artist + 0.20 * session + 0.10 * novelty + 0.05 * explore
            - 1.2 * veto;
        let mut reason = self.reason_for(tags);
        if veto > 0.5 {
            reason = if reason.is_empty() {
                format!("session veto ({veto:.1})")
            } else {
                format!("session veto ({veto:.1}) · {reason}")
            };
        }
        ScoreParts {
            tag,
            artist,
            session,
            novelty,
            total,
            primary_tag: primary,
            reason,
        }
    }

    /// Weight of a tag in the long-term profile, shifted by the session and
    /// hard-suppressed when the session has clearly rejected it.
    fn effective_tag(&self, tag: &str) -> f64 {
        let long = *self.long_term.tags.get(tag).unwrap_or(&0.0);
        let session = *self.session.tags.get(tag).unwrap_or(&0.0);
        let combined = long + 1.5 * session;
        if session <= -2.0 {
            combined.min(0.0) - 1.0
        } else {
            combined
        }
    }

    fn tag_similarity(&self, tags: &HashMap<String, f64>) -> f64 {
        let mut dot = 0.0;
        let mut candidate_norm = 0.0;
        for (tag, weight) in tags {
            candidate_norm += weight * weight;
            dot += weight * self.effective_tag(tag);
        }
        let mut profile_norm = 0.0;
        let mut keys: HashSet<&String> = HashSet::new();
        keys.extend(self.long_term.tags.keys());
        keys.extend(self.session.tags.keys());
        for key in keys {
            let weight = self.effective_tag(key);
            profile_norm += weight * weight;
        }
        if candidate_norm <= 0.0 || profile_norm <= 0.0 {
            return 0.0;
        }
        (dot / (candidate_norm.sqrt() * profile_norm.sqrt())).clamp(-1.0, 1.0)
    }

    fn session_similarity(&self, tags: &HashMap<String, f64>) -> f64 {
        let mut dot = 0.0;
        let mut candidate_norm = 0.0;
        for (tag, weight) in tags {
            candidate_norm += weight * weight;
            dot += weight * self.session.tags.get(tag).copied().unwrap_or(0.0).max(0.0);
        }
        let mut profile_norm = 0.0;
        for weight in self.session.tags.values() {
            let positive = weight.max(0.0);
            profile_norm += positive * positive;
        }
        if candidate_norm <= 0.0 || profile_norm <= 0.0 {
            return 0.0;
        }
        (dot / (candidate_norm.sqrt() * profile_norm.sqrt())).clamp(0.0, 1.0)
    }

    fn artist_similarity(&self, artist: &str) -> f64 {
        let key = artist_key(artist);
        if key.is_empty() {
            return 0.0;
        }
        let long = *self.long_term.artists.get(&key).unwrap_or(&0.0);
        let session = *self.session.artists.get(&key).unwrap_or(&0.0);
        let raw = (long + 0.8 * session).max(0.0);
        raw / (raw + 3.0)
    }

    fn reason_for(&self, tags: &HashMap<String, f64>) -> String {
        let mut matched: Vec<(&String, f64)> = tags
            .iter()
            .filter_map(|(tag, weight)| {
                let effective = self.effective_tag(tag);
                (effective.abs() > 0.2).then_some((tag, weight * effective))
            })
            .collect();
        matched.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        matched
            .into_iter()
            .take(3)
            .map(|(tag, score)| format!("{tag} {score:+.1}"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn confidence(&self) -> f64 {
        (self.session.completed as f64 + 1.0) / (self.session.plays as f64 + 2.0)
    }

    /// Tags that currently drive the profile, long-term and session combined.
    pub fn top_tags(&self, limit: usize) -> Vec<(String, f64)> {
        let mut combined: HashMap<String, f64> = HashMap::new();
        for (tag, weight) in &self.long_term.tags {
            *combined.entry(tag.clone()).or_insert(0.0) += *weight;
        }
        for (tag, weight) in &self.session.tags {
            *combined.entry(tag.clone()).or_insert(0.0) += 1.5 * *weight;
        }
        let mut out: Vec<(String, f64)> = combined
            .into_iter()
            .filter(|(_, weight)| *weight > 0.3)
            .collect();
        out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        out.truncate(limit);
        out
    }

    fn top_effective_tags(&self, limit: usize) -> Vec<(String, f64)> {
        let mut keys: HashSet<&String> = HashSet::new();
        keys.extend(self.long_term.tags.keys());
        keys.extend(self.session.tags.keys());
        let mut out: Vec<(String, f64)> = keys
            .into_iter()
            .map(|tag| (tag.clone(), self.effective_tag(tag)))
            .filter(|(_, weight)| *weight > 0.2)
            .collect();
        out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        out.truncate(limit);
        out
    }

    pub fn top_artists(&self, limit: usize) -> Vec<(String, f64)> {
        let mut combined: HashMap<String, f64> = HashMap::new();
        for (artist, weight) in &self.long_term.artists {
            *combined.entry(artist.clone()).or_insert(0.0) += *weight;
        }
        for (artist, weight) in &self.session.artists {
            *combined.entry(artist.clone()).or_insert(0.0) += 0.5 * *weight;
        }
        let mut out: Vec<(String, f64)> = combined
            .into_iter()
            .filter(|(_, weight)| *weight > 0.5)
            .collect();
        out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        out.truncate(limit);
        out
    }

    /// A tag adjacent to the current session, used for the mismatch probe.
    fn adjacent_tag(&self) -> Option<String> {
        // Only tags that came from genre metadata or plugin tags are probe
        // candidates. Title words like "midnight" are useful for similarity,
        // but probing them would search for something nobody calls a genre.
        let mut session: Vec<(&String, &f64)> = self
            .session
            .tags
            .iter()
            .filter(|(tag, weight)| **weight > 0.5 && self.known_tags.contains(*tag))
            .collect();
        if session.len() >= 2 {
            session.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
            return Some(session[1].0.clone());
        }
        let top: HashSet<String> = self
            .top_effective_tags(2)
            .into_iter()
            .map(|(tag, _)| tag)
            .collect();
        let mut long: Vec<(&String, &f64)> = self
            .long_term
            .tags
            .iter()
            .filter(|(tag, weight)| **weight > 1.0 && !top.contains(*tag) && self.known_tags.contains(*tag))
            .collect();
        long.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
        long.first().map(|(tag, _)| (*tag).clone())
    }

    pub fn summary(&self) -> String {
        let tags = self.top_tags(6);
        let artists = self.top_artists(4);
        let tag_line = if tags.is_empty() {
            "still learning".to_string()
        } else {
            tags.iter()
                .map(|(tag, weight)| format!("{tag} {weight:.1}"))
                .collect::<Vec<_>>()
                .join(" · ")
        };
        let artist_line = artists
            .iter()
            .map(|(artist, _)| artist.clone())
            .collect::<Vec<_>>()
            .join(" · ");
        let mut out = String::new();
        out.push_str(&format!(
            "session {} · {} plays · {} completed · {} skipped · confidence {:.0}%\n",
            self.session.id,
            self.session.plays,
            self.session.completed,
            self.session.skips,
            self.confidence() * 100.0
        ));
        out.push_str(&format!("top tags: {tag_line}\n"));
        if !artist_line.is_empty() {
            out.push_str(&format!("top artists: {artist_line}\n"));
        }
        out.push_str(&format!(
            "probes: {} tried · {} stuck · {} skipped\n",
            self.probes.trials, self.probes.successes, self.probes.failures
        ));
        if let Some(last) = &self.last_pick {
            out.push_str(&format!(
                "last pick: {} — {}  (tags {:+.2} · artist {:+.2} · session {:+.2} · total {:+.2})\n",
                last.title,
                last.artist,
                last.tag_score,
                last.artist_score,
                last.session_score,
                last.total
            ));
        }
        out.push_str("reset with `mfe taste reset`");
        out
    }

    pub fn explain_last(&self) -> String {
        let Some(last) = &self.last_pick else {
            return "no autoplay pick yet — play a few tracks first".to_string();
        };
        let mut out = format!(
            "{} — {}\n  tags {:+.3} · artist {:+.3} · session {:+.3} · novelty {:+.3} → {:+.3}\n",
            last.title,
            last.artist,
            last.tag_score,
            last.artist_score,
            last.session_score,
            last.novelty,
            last.total
        );
        if !last.reason.is_empty() {
            out.push_str(&format!("  matched: {}\n", last.reason));
        }
        out.push_str(&format!("  chosen {} (unix)\n", last.at));
        out
    }

    pub fn reset(&mut self, scope: &str) {
        match scope {
            "session" => {
                self.session = Session::default();
            }
            "probes" => {
                self.probes = Probes::default();
            }
            _ => {
                self.long_term = Affinity::default();
                self.session = Session::default();
                self.events.clear();
                self.probes = Probes::default();
                self.blocked.clear();
                self.known_tags.clear();
                self.last_pick = None;
                // Keep the profile from being silently rebuilt from history.
                self.seeded = true;
            }
        }
        self.dirty = true;
        self.updated_at = now_unix();
    }

    fn trim_events(&mut self) {
        while self.events.len() > EVENT_CAP {
            self.events.pop_back();
        }
    }

    fn clamp_maps(&mut self) {
        for weight in self.long_term.tags.values_mut() {
            *weight = weight.clamp(-50.0, 500.0);
        }
        for weight in self.session.tags.values_mut() {
            *weight = weight.clamp(-30.0, 200.0);
        }
        self.long_term.tags.retain(|_, weight| weight.abs() > 0.01);
        self.session.tags.retain(|_, weight| weight.abs() > 0.01);
    }

    fn decay(&mut self, now: u64) {
        if self.updated_at == 0 {
            self.updated_at = now;
        } else if now > self.updated_at {
            let days = (now - self.updated_at) as f64 / 86_400.0;
            if days >= 1.0 / 48.0 {
                let factor = 0.5f64.powf(days / LONG_TERM_HALF_LIFE_DAYS);
                for weight in self.long_term.tags.values_mut() {
                    *weight *= factor;
                }
                for weight in self.long_term.artists.values_mut() {
                    *weight *= factor;
                }
                self.updated_at = now;
            }
        }

        if self.session.last_at == 0 {
            self.session.last_at = now;
            return;
        }
        let idle = now.saturating_sub(self.session.last_at);
        if idle >= SESSION_IDLE_RESET_SECS {
            self.session = Session::default();
            return;
        }
        if idle >= 30 {
            let minutes = idle as f64 / 60.0;
            let factor = 0.5f64.powf(minutes / SESSION_HALF_LIFE_MINUTES);
            for weight in self.session.tags.values_mut() {
                *weight *= factor;
            }
            for weight in self.session.artists.values_mut() {
                *weight *= factor;
            }
            self.session.last_at = now;
        }
    }

    #[cfg(test)]
    fn force_updated_at(&mut self, at: u64) {
        self.updated_at = at;
    }
}

// ---------------------------------------------------------------------------
// tag derivation
// ---------------------------------------------------------------------------

/// Tags that literally came from metadata: genres and plugin tags, never
/// derived from the title. Used to decide what is worth probing.
fn explicit_tags(track: &Track) -> HashSet<String> {
    let mut out = HashSet::new();
    for raw in track.genre.iter().chain(track.tags.iter()) {
        for part in raw.split([',', '/', '&', ';']) {
            let tag = canonical_tag(part);
            if tag.len() >= 2 {
                out.insert(tag);
            }
        }
    }
    out
}

/// Tags for a track: explicit genres first, then plugin tags, then title words.
pub fn tags_for(track: &Track) -> HashMap<String, f64> {
    let mut out: HashMap<String, f64> = HashMap::new();
    let mut add = |raw: &str, weight: f64| {
        let tag = canonical_tag(raw);
        if tag.len() >= 2 {
            *out.entry(tag).or_insert(0.0) += weight;
        }
    };

    for genre in &track.genre {
        for part in genre.split([',', '/', '&', ';']) {
            add(part, 1.2);
        }
    }
    for tag in &track.tags {
        add(tag, 0.9);
    }
    for token in tokenize(&track.title) {
        add(&token, 0.3);
    }
    for token in tokenize(&track.album) {
        if token.len() > 3 {
            add(&token, 0.1);
        }
    }
    if !track.source.trim().is_empty() {
        let source = canonical_tag(&format!("src {}", track.source));
        add(&source, 0.2);
    }
    if let Some(year) = track.year.as_deref().filter(|year| year.len() == 4) {
        add(&format!("year {}", &year[..3]), 0.1);
    }
    out.retain(|_, weight| *weight > 0.05);
    out
}

fn canonical_tag(raw: &str) -> String {
    let mut tag = raw
        .trim()
        .to_ascii_lowercase()
        .replace(['_', '.', '\u{2019}'], " ")
        .replace('&', " and ");
    tag = tag
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || ch == ' ' || ch == '-' {
                ch
            } else {
                ' '
            }
        })
        .collect();
    tag = tag.split_whitespace().collect::<Vec<_>>().join(" ");
    let tag = match tag.as_str() {
        "drift phonk" | "brazilian phonk" | "house phonk" | "phonk house" | "aggressive phonk" => {
            "phonk"
        }
        "lo-fi" | "lofi" | "lofi hip hop" | "chillhop" => "lofi",
        "dnb" | "drum n bass" | "drum and bass" => "drum and bass",
        "hip-hop" | "hip hop" | "rap" => "hip hop",
        "r&b" | "rnb" | "r and b" => "rnb",
        "electronic dance music" | "edm" => "edm",
        "slowed and reverb" | "slowed reverb" => "slowed reverb",
        other => other,
    };
    tag.to_string()
}

fn tokenize(raw: &str) -> Vec<String> {
    const STOP: &[&str] = &[
        "the", "and", "for", "with", "from", "this", "that", "you", "your", "our", "are", "was",
        "were", "his", "her", "its", "not", "but", "all", "can", "out", "get", "got", "one", "two",
        "official", "video", "audio", "lyrics", "lyric", "feat", "ft", "prod", "prodby", "visualizer",
        "hd", "hq", "explicit", "album", "single", "version", "edit", "full", "song", "music",
        "mix", "playlist", "best", "new", "free", "download", "original",
    ];
    raw.to_ascii_lowercase()
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| word.len() >= 3 && word.len() <= 24)
        .filter(|word| !STOP.contains(word))
        .filter(|word| !word.chars().all(|ch| ch.is_ascii_digit()))
        .map(|word| canonical_tag(word))
        .filter(|word| !word.is_empty())
        .collect()
}

fn primary_tag(tags: &HashMap<String, f64>) -> Option<String> {
    tags.iter()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(tag, _)| tag.clone())
}

fn artist_key(artist: &str) -> String {
    artist.trim().to_ascii_lowercase()
}

fn jaccard(a: &HashMap<String, f64>, b: &HashMap<String, f64>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let mut intersection = 0.0;
    for (tag, weight) in a {
        if let Some(other) = b.get(tag) {
            intersection += weight.min(*other);
        }
    }
    let mut union = 0.0;
    for weight in a.values() {
        union += *weight;
    }
    for (tag, weight) in b {
        if !a.contains_key(tag) {
            union += *weight;
        }
    }
    if union <= 0.0 {
        0.0
    } else {
        intersection / union
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: &str, title: &str, artist: &str) -> Track {
        Track {
            id: id.to_string(),
            title: title.to_string(),
            artist: artist.to_string(),
            album: String::new(),
            duration: Some(200),
            artwork: None,
            source: "youtube".to_string(),
            year: None,
            genre: Vec::new(),
            tags: Vec::new(),
            auto_queued: false,
        }
    }

    fn phonk(id: &str) -> Track {
        let mut track = track(id, "Midnight Phonk Mix", "DJ Test");
        track.genre = vec!["Drift Phonk".to_string()];
        track
    }

    fn jazz(id: &str, artist: &str) -> Track {
        let mut track = track(id, "Blue in Green", artist);
        track.genre = vec!["Jazz".to_string()];
        track
    }

    fn profile_with(phonk_weight: f64) -> Taste {
        let mut taste = Taste::default();
        taste.long_term.tags.insert("phonk".to_string(), phonk_weight);
        taste.long_term.tags.insert("src youtube".to_string(), 2.0);
        taste.long_term.artists.insert("dj test".to_string(), 4.0);
        taste.known_tags.insert("phonk".to_string());
        taste.seeded = true;
        taste
    }

    #[test]
    fn long_term_halves_in_thirty_days() {
        let mut taste = profile_with(10.0);
        let now = now_unix();
        taste.force_updated_at(now - 30 * 86_400);
        taste.decay(now);
        let weight = *taste.long_term.tags.get("phonk").unwrap();
        assert!((weight - 5.0).abs() < 0.2, "expected ~5, got {weight}");
    }

    #[test]
    fn drift_phonk_normalises_to_phonk() {
        let tags = tags_for(&phonk("a"));
        assert!(tags.contains_key("phonk"), "tags: {tags:?}");
        assert!(!tags.contains_key("drift phonk"));
    }

    #[test]
    fn tag_words_come_from_the_title() {
        let tags = tags_for(&track("a", "Phonk Night Drive", "Someone"));
        assert!(tags.contains_key("phonk"));
        assert!(tags.contains_key("night"));
        assert!(tags.contains_key("drive"));
    }

    #[test]
    fn taste_prefers_the_matching_tag() {
        let mut taste = profile_with(10.0);
        let mut candidates = vec![jazz("j1", "Miles"), phonk("p1")];
        candidates.push(jazz("j2", "Coltrane"));
        let picks = taste.rank(&candidates, &[], 2);
        assert_eq!(picks[0].id, "p1");
    }

    #[test]
    fn five_quick_skips_suppress_phonk_for_the_session() {
        let mut taste = profile_with(40.0);
        for index in 0..5 {
            let track = phonk(&format!("skip-{index}"));
            taste.begin(&track, false, None);
            taste.progress(5.0);
            taste.skip(5.0);
        }
        let mut candidates = Vec::new();
        for index in 0..6 {
            candidates.push(phonk(&format!("p-{index}")));
        }
        // Enough alternative tracks that a suppressed profile can fill the
        // whole batch without falling back on phonk.
        for index in 0..12 {
            candidates.push(jazz(&format!("j-{index}"), &format!("Jazz {index}")));
        }
        let picks = taste.rank(&candidates, &[], 10);
        let phonk_count = picks
            .iter()
            .filter(|track| track.title.contains("Phonk"))
            .count();
        assert!(
            phonk_count * 10 < picks.len() * 3,
            "phonk should be under 30%, got {phonk_count}/{}",
            picks.len()
        );
    }

    #[test]
    fn completions_raise_the_tag_weight() {
        let mut taste = Taste::default();
        taste.seeded = true;
        for index in 0..3 {
            let track = phonk(&format!("p-{index}"));
            taste.begin(&track, false, None);
            taste.progress(199.0);
            taste.complete();
        }
        let weight = *taste.session.tags.get("phonk").unwrap_or(&0.0);
        assert!(weight > 1.0, "session phonk weight {weight}");
    }

    #[test]
    fn probe_needs_confidence_and_spacing() {
        let mut taste = profile_with(5.0);
        // A secondary tag outside the top two is what a probe would try.
        taste.long_term.tags.insert("lofi".to_string(), 1.6);
        taste.known_tags.insert("lofi".to_string());
        assert_eq!(taste.adjacent_tag().as_deref(), Some("lofi"));
        // Not enough plays yet.
        assert!(taste.session.plays < 8);
        let candidates: Vec<Track> = (0..4).map(|index| phonk(&format!("p-{index}"))).collect();
        let plan = taste.plan_batch(&candidates, &[], 4, true, 1);
        assert!(plan.probes.is_empty());
    }

    #[test]
    fn probe_fires_after_enough_plays() {
        let mut taste = profile_with(5.0);
        taste.long_term.tags.insert("lofi".to_string(), 1.6);
        taste.known_tags.insert("lofi".to_string());
        for index in 0..8 {
            let track = phonk(&format!("play-{index}"));
            taste.begin(&track, true, None);
            taste.progress(199.0);
            taste.complete();
        }
        assert!(taste.confidence() >= 0.55);
        assert!(taste.session.since_probe >= 1);

        let mut candidates: Vec<Track> = (0..6).map(|index| phonk(&format!("p-{index}"))).collect();
        for index in 0..4 {
            let mut track = jazz(&format!("l-{index}"), &format!("Lofi {index}"));
            track.genre = vec!["Lofi".to_string()];
            candidates.push(track);
        }
        let plan = taste.plan_batch(&candidates, &[], 4, true, 1);
        assert_eq!(plan.probes.len(), 1, "expected exactly one probe");
        let (id, tag) = plan.probes.iter().next().unwrap();
        assert_eq!(tag, "lofi");
        let probe = plan
            .tracks
            .iter()
            .find(|track| &track.id == id)
            .expect("probe track is in the batch");
        assert!(probe.genre.iter().any(|genre| genre == "Lofi"));
    }

    #[test]
    fn dislike_blocks_the_track() {
        let mut taste = Taste::default();
        taste.seeded = true;
        let bad = jazz("bad", "Someone");
        taste.dislike(&bad);
        let mut candidates = vec![bad.clone()];
        candidates.push(phonk("ok"));
        let picks = taste.rank(&candidates, &[], 1);
        assert_eq!(picks[0].id, "ok");
    }
}
