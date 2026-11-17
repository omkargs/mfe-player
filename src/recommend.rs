//! The local recommendation stack.
//!
//! Spotify's real system is a layered stack: implicit feedback feeds a latent
//! factor model, content and playlist graphs retrieve candidates, a contextual
//! session vector re-weights them, a bandit re-ranks, and a scored shuffle
//! decides the order. None of that needs a server to be useful, but the
//! *shape* of it does need to be honest about what a single machine can know.
//!
//! This module implements the same stages with the data a local player
//! actually has:
//!
//! | Spotify layer | Here |
//! |---|---|
//! | Implicit feedback | [`Signals::observe_outcome`] — plays, completions, skips, likes |
//! | Logistic MF / ANN retrieval | online logistic model over tag + artist features + exact cosine (the catalogue is already in memory) |
//! | Playlist co-occurrence graph | [`Signals::cooccur`] / [`Signals::artist_graph`] built from consecutive plays |
//! | CoSeRNN session + context | [`Context`] (time of day, weekday) + session affinity in `taste.rs` |
//! | BaRT exploration | [`Signals::thompson`] over genre arms with an epsilon-greedy novelty bonus |
//! | Quality / slop filter | [`is_low_quality`] |
//! | Scored shuffle | [`shuffle_upcoming`] / [`choose_next`] |
//!
//! Two things are deliberately *not* claimed: there are no other users'
//! vectors, and there is no audio CNN. Tags, metadata and this machine's own
//! co-listening graph are the content features instead. See
//! `docs/RECOMMENDATIONS.md` for the mapping and its limits.

use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::models::Track;
use crate::taste::{artist_key, tags_for, Affinity, Session};

/// 20–30% of the playlist is the spacing Spotify optimises for in its shuffle
/// patent. We use the middle of that range and let the score penalise both
/// clumping and perfectly regular spacing.
const ARTIST_SPACING: f64 = 0.25;
/// How many candidate tracks to look back for co-listening edges.
const BEHAVIOUR_MEMORY: usize = 12;

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// context
// ---------------------------------------------------------------------------

/// CoSeRNN-style context: day of week + hour of day. Device is always the
/// terminal, so it carries no information locally and is omitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    pub key: String,
    pub label: &'static str,
    pub hour: u32,
    pub weekend: bool,
}

/// Buckets from the CoSeRNN paper (early morning / morning / afternoon /
/// evening / late evening / night). Weekday and weekend share the buckets but
/// are separate keys, because a Sunday-morning session is not a Monday one.
pub fn bucket_for(hour: u32) -> &'static str {
    match hour {
        6..=8 => "early morning",
        9..=11 => "morning",
        12..=16 => "afternoon",
        17..=20 => "evening",
        21..=23 | 0 => "late evening",
        _ => "night",
    }
}

pub fn context_for(hour: u32, weekend: bool) -> Context {
    let label = bucket_for(hour);
    Context {
        key: format!("{}-{}", if weekend { "weekend" } else { "weekday" }, label),
        label,
        hour,
        weekend,
    }
}

/// The current listening context. Unix gets the real local hour; Windows
/// falls back to UTC unless `MFE_CONTEXT_HOUR` is set, which is honest and
/// keeps the crate dependency-free.
pub fn current_context() -> Context {
    let hour = std::env::var("MFE_CONTEXT_HOUR")
        .ok()
        .and_then(|raw| raw.trim().parse::<u32>().ok())
        .filter(|hour| *hour < 24)
        .unwrap_or_else(local_hour);
    context_for(hour, local_weekend())
}

#[cfg(unix)]
fn local_hour() -> u32 {
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            return utc_hour();
        }
        tm.tm_hour.clamp(0, 23) as u32
    }
}

#[cfg(not(unix))]
fn local_hour() -> u32 {
    utc_hour()
}

fn utc_hour() -> u32 {
    ((now_unix() / 3_600) % 24) as u32
}

#[cfg(unix)]
fn local_weekend() -> bool {
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            return false;
        }
        matches!(tm.tm_wday, 0 | 6)
    }
}

#[cfg(not(unix))]
fn local_weekend() -> bool {
    // 1970-01-01 was a Thursday: day 0 is Thursday, so (days + 4) % 7 is
    // Sunday-based.
    let days = now_unix() / 86_400;
    matches!((days + 4) % 7, 0 | 6)
}

// ---------------------------------------------------------------------------
// quality filter
// ---------------------------------------------------------------------------

/// Reasons a title/artist looks like recommendation spam. These are the
/// categories Spotify started excluding in 2026 (slowed/sped-up re-uploads,
/// nightcore, 8D edits, generated "AI covers"). Genuine remixes are untouched.
pub fn low_quality_reason(track: &Track) -> Option<&'static str> {
    let haystack = format!("{} {} {}", track.title, track.artist, track.album).to_lowercase();
    if haystack.contains("slowed") && (haystack.contains("reverb") || haystack.contains("reverbed")) {
        return Some("slowed/reverb re-upload");
    }
    const PATTERNS: &[(&str, &str)] = &[
        ("slowed down", "slowed/reverb re-upload"),
        ("slowed edit", "slowed/reverb re-upload"),
        ("slowed version", "slowed/reverb re-upload"),
        ("slowed remix", "slowed/reverb re-upload"),
        ("sped up", "sped-up re-upload"),
        ("speed up", "sped-up re-upload"),
        ("sped-up", "sped-up re-upload"),
        ("speed-up", "sped-up re-upload"),
        ("spedup", "sped-up re-upload"),
        ("nightcore", "nightcore edit"),
        ("daycore", "nightcore/daycore edit"),
        ("8d audio", "8D edit"),
        ("8d version", "8D edit"),
        ("8d edit", "8D edit"),
        ("8d sound", "8D edit"),
        ("8d music", "8D edit"),
        ("chopped and screwed", "chopped & screwed re-upload"),
        ("chopped & screwed", "chopped & screwed re-upload"),
        ("chopped + screwed", "chopped & screwed re-upload"),
        ("bass boosted", "bass-boosted re-upload"),
        ("bassboosted", "bass-boosted re-upload"),
        ("bass boost", "bass-boosted re-upload"),
        ("bass-boosted", "bass-boosted re-upload"),
        ("1 hour loop", "loop"),
        ("one hour loop", "loop"),
        ("10 hours", "loop"),
        ("10 hour loop", "loop"),
        ("hour loop", "loop"),
        ("ai cover", "AI-generated upload"),
        ("ai-generated", "AI-generated upload"),
        ("ai generated", "AI-generated upload"),
        ("ai vocal", "AI-generated upload"),
        ("ai vocals", "AI-generated upload"),
        ("made with suno", "AI-generated upload"),
        ("suno ai", "AI-generated upload"),
        ("udio ai", "AI-generated upload"),
    ];
    PATTERNS
        .iter()
        .find(|(needle, _)| haystack.contains(needle))
        .map(|(_, reason)| *reason)
}

/// Continuous energy score in [0.0, 1.0].
/// High energy ~ 1.0 (metal, rock, drum-and-bass, techno, edm, punk, hyperpop, etc.)
/// Low energy ~ 0.0 (ambient, lofi, chill, acoustic, folk, classical, sleep, etc.)
/// Neutral unclassified = 0.5.
pub fn track_energy(tags: &HashMap<String, f64>) -> f64 {
    if tags.is_empty() {
        return 0.5;
    }
    const HIGH_ENERGY: &[&str] = &[
        "metal", "rock", "punk", "hardcore", "electronic", "techno", "edm", "dance",
        "drum-and-bass", "dnb", "breakbeat", "dubstep", "hyperpop", "trap", "drill",
        "phonk", "hip-hop", "rap", "industrial", "house", "pop", "club",
    ];
    const LOW_ENERGY: &[&str] = &[
        "ambient", "lofi", "chill", "chillout", "acoustic", "folk", "classical",
        "piano", "sleep", "meditation", "drone", "downtempo", "jazz", "soul",
        "blues", "ballad", "quiet", "singer-songwriter",
    ];
    let mut high_sum = 0.0;
    let mut low_sum = 0.0;
    for (tag, weight) in tags {
        let tag_lower = tag.to_lowercase();
        for &high in HIGH_ENERGY {
            if tag_lower.contains(high) {
                high_sum += weight;
                break;
            }
        }
        for &low in LOW_ENERGY {
            if tag_lower.contains(low) {
                low_sum += weight;
                break;
            }
        }
    }
    if high_sum == 0.0 && low_sum == 0.0 {
        return 0.5;
    }
    let diff = high_sum - low_sum;
    let total = high_sum + low_sum;
    let norm = diff / total;
    ((norm + 1.0) / 2.0).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// deterministic RNG
// ---------------------------------------------------------------------------

/// Small deterministic RNG. Reproducible tests matter more than statistical
/// perfection here; every value is only used to break ties or sample a bandit.
#[derive(Debug, Clone)]
pub struct Lcg {
    state: u64,
}

impl Lcg {
    pub fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x9e37_79b9_7f4a_7c15,
        }
    }

    pub fn from_clock(salt: u64) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0);
        Self::new(nanos ^ salt.wrapping_mul(0xbf58_476d_1ce4_e5b9))
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.state
    }

    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / ((1u64 << 53) as f64)
    }

    /// Rough standard normal from three uniforms (CLT). Good enough to sample
    /// a normal approximation of the Beta posterior.
    pub fn normal(&mut self) -> f64 {
        let sum = self.next_f64() + self.next_f64() + self.next_f64();
        (sum - 1.5) * 2.0
    }
}

// ---------------------------------------------------------------------------
// learned signals
// ---------------------------------------------------------------------------

/// One Beta-Bernoulli arm per genre/tag. `alpha` counts reward, `beta`
/// counts non-reward; both start at 1 so an unseen tag has a wide posterior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Arm {
    #[serde(default = "prior")]
    pub alpha: f64,
    #[serde(default = "prior")]
    pub beta: f64,
}

fn prior() -> f64 {
    1.0
}

impl Default for Arm {
    fn default() -> Self {
        Self {
            alpha: prior(),
            beta: prior(),
        }
    }
}

impl Arm {
    fn trials(&self) -> f64 {
        (self.alpha + self.beta - 2.0).max(0.0)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LastStarted {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub at: u64,
}

/// Everything the recommender learns beyond the long-term/session affinities.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Signals {
    /// Track → track co-listening weights (the local stand-in for a playlist
    /// co-occurrence graph).
    #[serde(default)]
    pub cooccur: HashMap<String, HashMap<String, f64>>,
    /// Artist → artist co-listening weights, so new tracks by related artists
    /// still get a behavioural boost.
    #[serde(default)]
    pub artist_graph: HashMap<String, HashMap<String, f64>>,
    /// Context bucket → learned affinity, the session-embedding substitute.
    #[serde(default)]
    pub contexts: HashMap<String, Affinity>,
    /// Genre arm posteriors (BaRT-style exploration).
    #[serde(default)]
    pub arms: HashMap<String, Arm>,
    /// Online logistic weights over tag/artist/context features. This is the
    /// local analogue of Logistic MF: one user vector, hashed features.
    #[serde(default)]
    pub weights: HashMap<String, f64>,
    #[serde(default)]
    pub bias: f64,
    #[serde(default)]
    pub starts: u64,
    #[serde(default)]
    pub rewards: u64,
    #[serde(default)]
    pub last_track: Option<LastStarted>,
    /// The track that was playing before the current one, kept until its
    /// outcome arrives so the co-listening edge can be strengthened.
    #[serde(default)]
    pub previous: Option<String>,
    #[serde(default)]
    pub previous_artist: String,
    /// Track id → unix time until which autoplay should avoid it after a
    /// second quick skip. Shorter than the 30-day dislike block.
    #[serde(default)]
    pub skipped: HashMap<String, u64>,
    /// Track → track directed transition weights P(B|A).
    #[serde(default)]
    pub transitions: HashMap<String, HashMap<String, f64>>,
    /// Consecutive quick skips in the current session (fatigue / recovery indicator).
    #[serde(default)]
    pub consecutive_skips: u32,
}

#[derive(Debug, Clone)]
pub struct Outcome<'a> {
    pub id: &'a str,
    pub artist: &'a str,
    pub tags: &'a HashMap<String, f64>,
    /// 0.0 on an immediate skip, 1.0 on a liked/finished track.
    pub reward: f64,
    pub context: &'a Context,
}

impl Signals {
    /// Link consecutive plays into the co-listening graph. Called at the start
    /// of every track so skips still produce an edge (with weak weight).
    pub fn observe_start(&mut self, track: &Track, tags: &HashMap<String, f64>) {
        self.starts = self.starts.saturating_add(1);
        let artist = artist_key(&track.artist);
        self.previous = self.last_track.as_ref().map(|last| last.id.clone());
        self.previous_artist = self
            .last_track
            .as_ref()
            .map(|last| last.artist.clone())
            .unwrap_or_default();
        if let Some(previous) = self.last_track.clone() {
            if !previous.id.is_empty() && previous.id != track.id {
                add_edge(&mut self.cooccur, &previous.id, &track.id, 0.4);
                add_edge(&mut self.cooccur, &track.id, &previous.id, 0.2);
                if !previous.artist.is_empty() && !artist.is_empty() && previous.artist != artist {
                    add_edge(&mut self.artist_graph, &previous.artist, &artist, 0.4);
                    add_edge(&mut self.artist_graph, &artist, &previous.artist, 0.2);
                }
            }
        }
        self.last_track = Some(LastStarted {
            id: track.id.clone(),
            artist,
            tags: tags.keys().cloned().collect(),
            at: now_unix(),
        });
    }

    /// Learn from how the track ended: logistic SGD, bandit arm update, and
    /// the context bucket's affinity. `reward` is continuous, which is the
    /// "streaming time" style reward from Spotify's bandit papers rather than
    /// a binary click.
    pub fn observe_outcome(&mut self, outcome: Outcome<'_>) {
        self.rewards = self.rewards.saturating_add(1);
        let tags = outcome.tags;
        let artist = artist_key(outcome.artist);
        let reward = outcome.reward.clamp(0.0, 1.0);

        // 1) Beta-Bernoulli arms for the primary genres.
        let mut arms: Vec<&String> = tags.keys().collect();
        arms.sort_by(|a, b| {
            tags.get(*b)
                .partial_cmp(&tags.get(*a))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for tag in arms.into_iter().take(2) {
            let arm = self.arms.entry(tag.clone()).or_default();
            arm.alpha += reward;
            arm.beta += 1.0 - reward;
        }

        // 2) Online logistic model (bias + one weight per feature).
        let p = self.logistic_for(tags, &artist, outcome.context);
        let error = reward - p;
        const RATE: f64 = 0.12;
        self.bias = (self.bias + RATE * error).clamp(-4.0, 4.0);
        for (tag, weight) in tags {
            self.bump(&format!("t:{tag}"), RATE * error * weight);
            // Context interaction: the same genre can be right at 8am and
            // wrong at midnight.
            self.bump(
                &format!("c:{}:{tag}", outcome.context.key),
                RATE * 0.5 * error * weight,
            );
        }
        if !artist.is_empty() {
            self.bump(&format!("a:{artist}"), RATE * error);
        }

        // 3) Context affinity (the CoSeRNN session vector).
        let context = self
            .contexts
            .entry(outcome.context.key.clone())
            .or_default();
        for (tag, weight) in tags {
            *context.tags.entry(tag.clone()).or_insert(0.0) += reward * weight;
        }
        if !artist.is_empty() {
            *context.artists.entry(artist.clone()).or_insert(0.0) += reward;
        }

        // 4) Strengthen the co-listening edge with the previous track.
        if let Some(previous) = self.previous.clone() {
            if !previous.is_empty() && previous != outcome.id {
                add_edge(&mut self.cooccur, &previous, outcome.id, 0.6 + reward);
                if !self.previous_artist.is_empty() && !artist.is_empty() {
                    add_edge(
                        &mut self.artist_graph,
                        &self.previous_artist,
                        &artist,
                        0.6 + reward,
                    );
                }
                if reward > 0.25 {
                    add_edge(&mut self.transitions, &previous, outcome.id, 0.5 + reward);
                }
            }
        }

        // 5) Track consecutive skips for fatigue / recovery mode.
        if reward <= 0.25 {
            self.consecutive_skips = self.consecutive_skips.saturating_add(1);
        } else if reward >= 0.70 {
            self.consecutive_skips = 0;
        }

        // 6) A second quick skip keeps a track out of autoplay for a while,
        // without permanently blacklisting it like a dislike does.
        if reward <= 0.05 && outcome.tags.values().any(|weight| *weight > 0.9) {
            let until = now_unix().saturating_add(7 * 24 * 60 * 60);
            self.skipped.insert(outcome.id.to_string(), until);
        }

        self.trim();
    }

    fn bump(&mut self, feature: &str, delta: f64) {
        if !delta.is_finite() || delta == 0.0 {
            return;
        }
        let weight = self.weights.entry(feature.to_string()).or_insert(0.0);
        *weight = (*weight + delta).clamp(-6.0, 6.0);
    }

    fn logistic_for(&self, tags: &HashMap<String, f64>, artist: &str, context: &Context) -> f64 {
        let mut raw = self.bias;
        for (tag, weight) in tags {
            raw += self
                .weights
                .get(&format!("t:{tag}"))
                .copied()
                .unwrap_or(0.0)
                * weight;
            raw += self
                .weights
                .get(&format!("c:{}:{tag}", context.key))
                .copied()
                .unwrap_or(0.0)
                * weight;
        }
        raw += self
            .weights
            .get(&format!("a:{artist}"))
            .copied()
            .unwrap_or(0.0);
        sigmoid(raw)
    }

    pub fn logistic(&self, tags: &HashMap<String, f64>, artist: &str, context: &Context) -> f64 {
        self.logistic_for(tags, &artist_key(artist), context)
    }

    /// Thompson-style sample from the arm's Beta posterior. `rng` keeps the
    /// batch reproducible within one ranking pass.
    pub fn thompson(&self, tag: &str, rng: &mut Lcg) -> f64 {
        let arm = self.arms.get(tag);
        let (alpha, beta) = arm.map(|arm| (arm.alpha, arm.beta)).unwrap_or((1.0, 1.0));
        let total = alpha + beta;
        let mean = alpha / total;
        let variance = (alpha * beta) / (total * total * (total + 1.0));
        (mean + rng.normal() * variance.sqrt()).clamp(0.0, 1.0)
    }

    /// Tags with wide posteriors, used by the deliberate out-of-profile probe.
    pub fn uncertain_tag(&self, known: &HashSet<String>) -> Option<String> {
        self.arms
            .iter()
            .filter(|(tag, _)| known.contains(*tag))
            .filter(|(_, arm)| arm.trials() < 6.0)
            .max_by(|a, b| {
                let spread = |arm: &Arm| {
                    let total = arm.alpha + arm.beta;
                    (arm.alpha * arm.beta) / (total * total * (total + 1.0))
                };
                spread(a.1)
                    .partial_cmp(&spread(b.1))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(tag, _)| tag.clone())
    }

    /// Behavioural similarity: has this listener played the candidate, or an
    /// artist adjacent to it, right before the recent tracks?
    pub fn behaviour(&self, track: &Track, recent: &[Track]) -> f64 {
        let mut best: f64 = 0.0;
        let artist = artist_key(&track.artist);
        for (index, previous) in recent.iter().take(BEHAVIOUR_MEMORY).enumerate() {
            let decay = 1.0 - index as f64 / (BEHAVIOUR_MEMORY as f64 + 2.0);
            if let Some(edges) = self.cooccur.get(&previous.id) {
                if let Some(weight) = edges.get(&track.id) {
                    best = best.max((weight / 4.0).min(1.0) * decay);
                }
            }
            if let Some(edges) = self.artist_graph.get(&artist_key(&previous.artist)) {
                if let Some(weight) = edges.get(&artist) {
                    best = best.max(0.65 * (weight / 4.0).min(1.0) * decay);
                }
            }
        }
        best
    }

    pub fn context_affinity(&self, context: &Context) -> Option<&Affinity> {
        self.contexts.get(&context.key)
    }

    /// Directed Markov transition probability P(to | from).
    pub fn markov_transition(&self, from: &str, to: &str) -> f64 {
        if from.is_empty() || to.is_empty() {
            return 0.0;
        }
        if let Some(targets) = self.transitions.get(from) {
            if let Some(&weight) = targets.get(to) {
                let total: f64 = targets.values().sum();
                if total > 0.0 {
                    return (weight / total).clamp(0.0, 1.0);
                }
            }
        }
        0.0
    }

    pub fn decay(&mut self, long_term_factor: f64) {
        let factor = long_term_factor.sqrt().max(0.5);
        for edges in self.cooccur.values_mut() {
            for weight in edges.values_mut() {
                *weight *= factor;
            }
            edges.retain(|_, weight| *weight > 0.02);
        }
        self.cooccur.retain(|_, edges| !edges.is_empty());
        for edges in self.artist_graph.values_mut() {
            for weight in edges.values_mut() {
                *weight *= factor;
            }
            edges.retain(|_, weight| *weight > 0.02);
        }
        self.artist_graph.retain(|_, edges| !edges.is_empty());
        for edges in self.transitions.values_mut() {
            for weight in edges.values_mut() {
                *weight *= factor;
            }
            edges.retain(|_, weight| *weight > 0.02);
        }
        self.transitions.retain(|_, edges| !edges.is_empty());
        for context in self.contexts.values_mut() {
            for weight in context.tags.values_mut() {
                *weight *= long_term_factor;
            }
            for weight in context.artists.values_mut() {
                *weight *= long_term_factor;
            }
            context.tags.retain(|_, weight| weight.abs() > 0.01);
            context.artists.retain(|_, weight| weight.abs() > 0.01);
        }
        for weight in self.weights.values_mut() {
            *weight *= factor;
        }
        let now = now_unix();
        self.skipped.retain(|_, until| *until > now);
        self.trim();
    }

    /// Bound memory. This is a personal player, not a data warehouse.
    fn trim(&mut self) {
        self.skipped.retain(|_, until| *until > now_unix());
        for edges in self.cooccur.values_mut() {
            trim_edges(edges, 32);
        }
        for edges in self.artist_graph.values_mut() {
            trim_edges(edges, 32);
        }
        for edges in self.transitions.values_mut() {
            trim_edges(edges, 32);
        }
        if self.transitions.len() > 3000 {
            let mut nodes: Vec<(String, f64)> = self
                .transitions
                .iter()
                .map(|(id, edges)| (id.clone(), edges.values().sum::<f64>()))
                .collect();
            nodes.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            for (id, _) in nodes.into_iter().take(self.transitions.len() - 3000) {
                self.transitions.remove(&id);
            }
        }
        if self.cooccur.len() > 3000 {
            let mut nodes: Vec<(String, f64)> = self
                .cooccur
                .iter()
                .map(|(id, edges)| (id.clone(), edges.values().sum::<f64>()))
                .collect();
            nodes.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            for (id, _) in nodes.into_iter().take(self.cooccur.len() - 3000) {
                self.cooccur.remove(&id);
            }
        }
        if self.weights.len() > 4000 {
            let mut features: Vec<(String, f64)> = self
                .weights
                .iter()
                .map(|(feature, weight)| (feature.clone(), weight.abs()))
                .collect();
            features.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            for (feature, _) in features.into_iter().take(self.weights.len() - 4000) {
                self.weights.remove(&feature);
            }
        }
    }

    pub fn summary(&self) -> String {
        let context = current_context();
        let mut out = format!(
            "context {} ({}h) · {} starts · {} learned outcomes\n",
            context.key, context.hour, self.starts, self.rewards
        );
        let mut arms: Vec<(&String, f64, f64)> = self
            .arms
            .iter()
            .filter(|(_, arm)| arm.trials() >= 1.0)
            .map(|(tag, arm)| (tag, arm.alpha / (arm.alpha + arm.beta), arm.trials()))
            .collect();
        arms.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        if !arms.is_empty() {
            let line = arms
                .iter()
                .take(6)
                .map(|(tag, mean, trials)| format!("{tag} {:.0}% ({trials:.0})", mean * 100.0))
                .collect::<Vec<_>>()
                .join(" · ");
            out.push_str(&format!("bandit arms: {line}\n"));
        }
        let context_line = self
            .contexts
            .get(&context.key)
            .map(|affinity| top_map_line(&affinity.tags, 4))
            .filter(|line| !line.is_empty());
        if let Some(line) = context_line {
            out.push_str(&format!("this context: {line}\n"));
        }
        out.push_str(&format!(
            "graph: {} tracks · {} artist links · {} features\n",
            self.cooccur.len(),
            self.artist_graph.len(),
            self.weights.len()
        ));
        out
    }
}

fn top_map_line(map: &HashMap<String, f64>, limit: usize) -> String {
    let mut entries: Vec<(&String, f64)> = map
        .iter()
        .filter(|(_, weight)| **weight > 0.2)
        .map(|(key, weight)| (key, *weight))
        .collect();
    entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    entries
        .into_iter()
        .take(limit)
        .map(|(key, weight)| format!("{key} {weight:.1}"))
        .collect::<Vec<_>>()
        .join(" · ")
}

fn add_edge(graph: &mut HashMap<String, HashMap<String, f64>>, from: &str, to: &str, weight: f64) {
    if from.is_empty() || to.is_empty() || from == to {
        return;
    }
    *graph
        .entry(from.to_string())
        .or_default()
        .entry(to.to_string())
        .or_insert(0.0) += weight;
}

fn trim_edges(edges: &mut HashMap<String, f64>, cap: usize) {
    if edges.len() <= cap {
        return;
    }
    let mut ranked: Vec<(String, f64)> = edges
        .iter()
        .map(|(id, weight)| (id.clone(), *weight))
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(cap);
    *edges = ranked.into_iter().collect();
}

fn sigmoid(raw: f64) -> f64 {
    if raw >= 0.0 {
        1.0 / (1.0 + (-raw).exp())
    } else {
        let exp = raw.exp();
        exp / (1.0 + exp)
    }
}

// ---------------------------------------------------------------------------
// scoring
// ---------------------------------------------------------------------------

/// Everything the scoring pass needs from the taste profile. Borrowed so a
/// ranking pass never clones the whole profile.
pub struct Profile<'a> {
    pub long_term: &'a Affinity,
    pub session: &'a Session,
    pub signals: &'a Signals,
    pub context: &'a Context,
    /// Track id → last unix play, for the freshness term.
    pub played_at: &'a HashMap<String, u64>,
    pub recent_artists: &'a HashSet<String>,
    /// Most recent queue tracks, newest last. Feeds the co-listening graph.
    pub recent: &'a [Track],
    pub last_primary: Option<&'a str>,
    /// Epsilon-greedy share (BaRT). 0.0 = pure exploitation.
    pub explore: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Breakdown {
    pub content: f64,
    pub artist: f64,
    pub behavior: f64,
    pub context: f64,
    pub logistic: f64,
    pub bandit: f64,
    pub freshness: f64,
    pub novelty: f64,
    pub pacing: f64,
    pub transition: f64,
    pub total: f64,
    pub primary_tag: String,
    pub reason: String,
    pub quality: Option<&'static str>,
}

/// One candidate's score. All components are 0..1 (or penalties), which
/// makes `mfe taste explain` readable instead of a wall of magic numbers.
pub fn score(
    profile: &Profile<'_>,
    track: &Track,
    tags: &HashMap<String, f64>,
    rng: &mut Lcg,
) -> Breakdown {
    let now = now_unix();
    let quality = low_quality_reason(track);
    let content = cosine(tags, &profile.long_term.tags);
    let context = cosine(
        tags,
        profile
            .signals
            .context_affinity(profile.context)
            .map(|affinity| &affinity.tags)
            .unwrap_or(&profile.session.tags),
    )
    .max(0.0);
    let artist = artist_affinity(profile.long_term, profile.session, &track.artist);
    let behavior = profile.signals.behaviour(track, profile.recent);
    let logistic = profile
        .signals
        .logistic(tags, &track.artist, profile.context);
    let primary = primary_tag(tags).unwrap_or_default();

    let in_recovery = profile.signals.consecutive_skips >= 2;
    let effective_explore = if in_recovery { 0.0 } else { profile.explore };

    let mut bandit = profile.signals.thompson(&primary, rng);
    // Epsilon-greedy: give under-explored arms a chance to prove themselves.
    let explore_bonus = if rng.next_f64() < effective_explore {
        let trials = profile
            .signals
            .arms
            .get(&primary)
            .map(Arm::trials)
            .unwrap_or(0.0);
        if trials < 8.0 {
            0.18
        } else {
            0.04
        }
    } else {
        0.0
    };
    bandit = (bandit + explore_bonus).clamp(0.0, 1.0);

    // Energy pacing & DJ flow continuity:
    let cand_energy = track_energy(tags);
    let prev_track = profile.recent.first();
    let prev_energy = prev_track
        .map(|t| track_energy(&tags_for(t)))
        .or_else(|| {
            profile.signals.last_track.as_ref().map(|l| {
                let m: HashMap<String, f64> = l.tags.iter().map(|t| (t.clone(), 1.0)).collect();
                track_energy(&m)
            })
        });
    let pacing = if let Some(prev_e) = prev_energy {
        let diff = (prev_e - cand_energy).abs();
        if diff > 0.70 {
            -0.15 * ((diff - 0.70) / 0.30)
        } else if diff <= 0.35 {
            0.06 * (1.0 - diff / 0.35)
        } else {
            0.0
        }
    } else {
        0.0
    };

    // Directed Markov transition probability P(candidate | previous):
    let prev_id = prev_track
        .map(|t| t.id.as_str())
        .or_else(|| profile.signals.last_track.as_ref().map(|l| l.id.as_str()));
    let transition = prev_id
        .map(|id| profile.signals.markov_transition(id, &track.id))
        .unwrap_or(0.0);

    // Skip-streak fatigue recovery anchor boost:
    let anchor_bonus = if in_recovery {
        if content >= 0.50 || artist >= 0.40 {
            0.28
        } else if content < 0.25 && artist < 0.15 {
            -0.20
        } else {
            0.0
        }
    } else {
        0.0
    };

    let freshness = freshness(profile.played_at.get(&track.id).copied(), now);
    let same_vibe = profile.last_primary.is_some_and(|last| last == primary);
    let artist_novel = !profile.recent_artists.contains(&artist_key(&track.artist));
    let novelty = if artist_novel {
        if same_vibe {
            0.6
        } else {
            1.0
        }
    } else {
        0.0
    };

    // A tag the listener has repeatedly skipped tonight must not sneak back in
    // just because the MMR pass wants diversity.
    let veto: f64 = tags
        .iter()
        .filter(|(tag, _)| {
            profile
                .session
                .tags
                .get(*tag)
                .is_some_and(|weight| *weight <= -2.0)
        })
        .map(|(_, weight)| *weight)
        .sum();

    let total = 0.22 * content
        + 0.15 * logistic
        + 0.13 * behavior
        + 0.12 * context
        + 0.09 * artist
        + 0.08 * transition
        + 0.06 * novelty
        + 0.05 * bandit
        + 0.10 * freshness
        + pacing
        + anchor_bonus
        - 1.2 * veto;

    let mut reason = reason_for(profile, tags);
    if in_recovery && anchor_bonus > 0.0 {
        reason = if reason.is_empty() {
            "recovery anchor".to_string()
        } else {
            format!("recovery anchor · {reason}")
        };
    }
    if veto > 0.5 {
        reason = if reason.is_empty() {
            format!("session veto ({veto:.1})")
        } else {
            format!("session veto ({veto:.1}) · {reason}")
        };
    }
    if let Some(quality) = quality {
        reason = format!("filtered: {quality}");
    }

    Breakdown {
        content,
        artist,
        behavior,
        context,
        logistic,
        bandit,
        freshness,
        novelty,
        pacing,
        transition,
        total,
        primary_tag: primary,
        reason,
        quality,
    }
}

/// Freshness 0 (just played) .. 1 (new or older than a month).
fn freshness(last_played: Option<u64>, now: u64) -> f64 {
    let Some(at) = last_played else {
        return 1.0;
    };
    let age = now.saturating_sub(at);
    const HOUR: u64 = 3_600;
    const DAY: u64 = 86_400;
    if age <= 6 * HOUR {
        0.1
    } else if age <= 48 * HOUR {
        0.35
    } else if age <= 7 * DAY {
        0.65
    } else if age <= 30 * DAY {
        0.85
    } else {
        1.0
    }
}

fn artist_affinity(long_term: &Affinity, session: &Session, artist: &str) -> f64 {
    let key = artist_key(artist);
    if key.is_empty() {
        return 0.0;
    }
    let long = *long_term.artists.get(&key).unwrap_or(&0.0);
    let session = *session.artists.get(&key).unwrap_or(&0.0);
    let raw = (long + 0.8 * session).max(0.0);
    raw / (raw + 3.0)
}

fn reason_for(profile: &Profile<'_>, tags: &HashMap<String, f64>) -> String {
    let mut matched: Vec<(&String, f64)> = tags
        .iter()
        .filter_map(|(tag, weight)| {
            let long = *profile.long_term.tags.get(tag).unwrap_or(&0.0);
            let session = *profile.session.tags.get(tag).unwrap_or(&0.0);
            let effective = long + 1.5 * session;
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

fn cosine(a: &HashMap<String, f64>, b: &HashMap<String, f64>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0;
    let mut norm_a = 0.0;
    for (tag, weight) in a {
        norm_a += weight * weight;
        dot += weight * b.get(tag).copied().unwrap_or(0.0);
    }
    let mut norm_b = 0.0;
    for weight in b.values() {
        norm_b += weight * weight;
    }
    if norm_a <= 0.0 || norm_b <= 0.0 {
        return 0.0;
    }
    (dot / (norm_a.sqrt() * norm_b.sqrt())).clamp(-1.0, 1.0)
}

fn primary_tag(tags: &HashMap<String, f64>) -> Option<String> {
    tags.iter()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(tag, _)| tag.clone())
}

/// Stable key for "same artist" comparisons.
pub fn same_artist(a: &Track, b: &Track) -> bool {
    let left = artist_key(&a.artist);
    !left.is_empty() && left == artist_key(&b.artist)
}

// ---------------------------------------------------------------------------
// scored shuffle
// ---------------------------------------------------------------------------

/// Score one candidate order. Freshness is weighted towards the front of the
/// queue (the first few tracks matter most), artists are spread ~25% of the
/// queue apart, and consecutive tracks should not share a genre tag.
pub fn sequence_score(order: &[Track], recent: &[Track], recent_ids: &HashSet<String>) -> f64 {
    if order.len() < 2 {
        return 0.0;
    }
    let len = order.len() as f64;
    let mut score = 0.0;

    for (index, track) in order.iter().enumerate() {
        let position = index as f64 / len;
        if recent_ids.contains(&track.id) {
            // Recently heard tracks are worst at the front of the queue.
            score -= 2.5 * (1.0 - position);
        }
        if let Some(last) = recent.first() {
            if same_artist(last, track) && index < 2 {
                score -= 0.8 * (2 - index) as f64;
            }
        }
        if index > 0 {
            let previous = &order[index - 1];
            if same_artist(previous, track) {
                score -= 2.0;
            }
            let left = tags_for(previous);
            let right = tags_for(track);
            if primary_tag(&left).is_some_and(|tag| Some(&tag) == primary_tag(&right).as_ref()) {
                score -= 0.45;
            }
        }
    }

    // Artist spacing: penalise gaps below 20% of the queue, with a gentle
    // reward around the 25% target. Not perfectly even, on purpose.
    let target = (ARTIST_SPACING * len).max(1.0);
    let mut positions: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, track) in order.iter().enumerate() {
        let key = artist_key(&track.artist);
        if !key.is_empty() {
            positions.entry(key).or_default().push(index);
        }
    }
    for slots in positions.values() {
        if slots.len() < 2 {
            continue;
        }
        for pair in slots.windows(2) {
            let gap = (pair[1] - pair[0]) as f64;
            if gap < target * 0.65 {
                score -= 1.5 * (1.0 - gap / (target * 0.65));
            } else if (gap - target).abs() <= target * 0.6 {
                score += 0.35;
            }
        }
    }

    score / len
}

/// Reorder the upcoming part of the queue using three sampled sequences and
/// keep the best-scoring one. Spotify's shuffle generates and scores several
/// candidate sequences; three keeps it instant on a terminal-sized queue.
pub fn shuffle_upcoming(queue: &mut [Track], current: usize, recent: &[Track], seed: u64) {
    let start = current.saturating_add(1);
    if queue.len().saturating_sub(start) < 2 {
        return;
    }
    let recent_ids: HashSet<String> = recent.iter().map(|track| track.id.clone()).collect();
    let remaining: Vec<Track> = queue[start..].to_vec();
    let mut best: Option<(f64, Vec<Track>)> = None;
    for attempt in 0..3u64 {
        let mut rng = Lcg::new(seed ^ attempt.wrapping_mul(0x2545_f491_4f6c_dd1d));
        let mut order = remaining.clone();
        fisher_yates(&mut order, &mut rng);
        let score = sequence_score(&order, recent, &recent_ids);
        if best.as_ref().is_none_or(|(seen, _)| score > *seen) {
            best = Some((score, order));
        }
    }
    if let Some((_, order)) = best {
        queue[start..].clone_from_slice(&order);
    }
}

/// Next index for shuffle mode: sample a few remaining tracks and keep the
/// one that fits best after the last few plays. This preserves the queue
/// order (unlike mutating it) while still avoiding artist clumps.
pub fn choose_next(queue: &[Track], current: usize, recent: &[Track], seed: u64) -> Option<usize> {
    let len = queue.len();
    if len <= 1 {
        return (len == 1 && current != 0).then_some(0);
    }
    let recent_ids: HashSet<String> = recent.iter().map(|track| track.id.clone()).collect();
    let mut rng = Lcg::new(seed);
    let mut best: Option<(f64, usize)> = None;
    // Four probes is enough to feel varied without making the choice feel
    // deterministic.
    for _ in 0..4.min(len.saturating_sub(1)) {
        let index = (rng.next_u64() % len as u64) as usize;
        if index == current {
            continue;
        }
        let mut ahead = vec![queue[index].clone()];
        if let Some(track) = queue.get(current) {
            ahead.insert(0, track.clone());
        }
        let score = sequence_score(&ahead, recent, &recent_ids) + rng.next_f64() * 0.25;
        if best.as_ref().is_none_or(|(seen, _)| score > *seen) {
            best = Some((score, index));
        }
    }
    best.map(|(_, index)| index)
        .or_else(|| Some((current + 1) % len))
}

fn fisher_yates(items: &mut [Track], rng: &mut Lcg) {
    if items.len() < 2 {
        return;
    }
    for index in (1..items.len()).rev() {
        let swap = (rng.next_u64() % (index as u64 + 1)) as usize;
        items.swap(index, swap);
    }
}

/// Reward curve for implicit feedback. A like is 1.0, a finished track is
/// 1.0, a half-listen 0.6, a soft skip 0.25, a quick skip 0.0. `None` means
/// the event says nothing (playback error).
pub fn reward_for(kind: crate::taste::EventKind, fraction: f64) -> Option<f64> {
    use crate::taste::EventKind;
    Some(match kind {
        EventKind::Like => 1.0,
        EventKind::Repeat => 1.0,
        EventKind::Complete => 1.0,
        EventKind::Half => 0.6,
        EventKind::SoftSkip => 0.25,
        EventKind::Skip => 0.05,
        EventKind::QuickSkip => 0.0,
        EventKind::Dislike => 0.0,
        EventKind::Error => return None,
    })
    .map(|reward| {
        // Completion fraction refines the bucket: a 95% listen is stronger
        // than a track that ended early due to a broken stream.
        if reward >= 0.6 {
            reward * (0.7 + 0.3 * fraction.clamp(0.0, 1.0))
        } else {
            reward
        }
    })
}

/// Co-listening edge weight, used by tests to assert graph updates.
#[cfg(test)]
pub fn edge_weight(signals: &Signals, from: &str, to: &str) -> f64 {
    signals
        .cooccur
        .get(from)
        .and_then(|edges| edges.get(to))
        .copied()
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taste::EventKind;

    fn track(id: &str, title: &str, artist: &str, genre: &str) -> Track {
        Track {
            id: id.to_string(),
            title: title.to_string(),
            artist: artist.to_string(),
            album: String::new(),
            duration: Some(200),
            artwork: None,
            source: "youtube".to_string(),
            year: None,
            genre: vec![genre.to_string()],
            tags: Vec::new(),
            auto_queued: false,
        }
    }

    fn tags(genre: &str) -> HashMap<String, f64> {
        let mut map = HashMap::new();
        map.insert(genre.to_string(), 1.0);
        map
    }

    #[test]
    fn quality_filter_flags_reuploads_but_not_remixes() {
        for title in [
            "Song (slowed + reverb)",
            "Nightcore - Song",
            "Song (sped up)",
            "Song (8D Audio)",
            "Song (AI cover)",
            "Song (1 hour loop)",
        ] {
            let candidate = track("x", title, "Someone", "pop");
            assert!(
                low_quality_reason(&candidate).is_some(),
                "{title} should be filtered"
            );
        }
        for title in ["Song (Extended Mix)", "Song (Remix)", "Song (Live)"] {
            let candidate = track("x", title, "Someone", "pop");
            assert!(
                low_quality_reason(&candidate).is_none(),
                "{title} should survive"
            );
        }
    }

    #[test]
    fn context_buckets_follow_the_cosernn_windows() {
        assert_eq!(bucket_for(7), "early morning");
        assert_eq!(bucket_for(10), "morning");
        assert_eq!(bucket_for(14), "afternoon");
        assert_eq!(bucket_for(18), "evening");
        assert_eq!(bucket_for(22), "late evening");
        assert_eq!(bucket_for(0), "late evening");
        assert_eq!(bucket_for(3), "night");
        let context = context_for(22, false);
        assert_eq!(context.key, "weekday-late evening");
        assert_eq!(context_for(22, true).key, "weekend-late evening");
    }

    #[test]
    fn consecutive_plays_build_the_co_listening_graph() {
        let mut signals = Signals::default();
        let first = track("a", "First", "One", "phonk");
        let second = track("b", "Second", "Two", "phonk");
        signals.observe_start(&first, &tags("phonk"));
        signals.observe_start(&second, &tags("phonk"));
        assert!(edge_weight(&signals, "a", "b") > 0.0);
        assert!(signals.artist_graph.contains_key("one"));
        // ...and rewards strengthen the edge.
        let before = edge_weight(&signals, "a", "b");
        let context = context_for(20, false);
        signals.observe_outcome(Outcome {
            id: "b",
            artist: "Two",
            tags: &tags("phonk"),
            reward: 1.0,
            context: &context,
        });
        assert!(edge_weight(&signals, "a", "b") > before);
    }

    #[test]
    fn finishing_a_tag_moves_its_bandit_posterior() {
        let mut signals = Signals::default();
        let context = context_for(20, false);
        let phonk = tags("phonk");
        for _ in 0..4 {
            signals.observe_outcome(Outcome {
                id: "p",
                artist: "DJ",
                tags: &phonk,
                reward: 1.0,
                context: &context,
            });
        }
        let arm = signals.arms.get("phonk").expect("arm exists");
        assert!((arm.alpha - 5.0).abs() < 1e-9, "alpha {}", arm.alpha);
        assert!(arm.alpha / (arm.alpha + arm.beta) > 0.8);

        let jazz = tags("jazz");
        for _ in 0..4 {
            signals.observe_outcome(Outcome {
                id: "j",
                artist: "Trio",
                tags: &jazz,
                reward: 0.0,
                context: &context,
            });
        }
        let arm = signals.arms.get("jazz").expect("arm exists");
        assert!(arm.alpha / (arm.alpha + arm.beta) < 0.3);
    }

    #[test]
    fn logistic_model_learns_liked_and_disliked_tags() {
        let mut signals = Signals::default();
        let context = context_for(9, false);
        let liked = tags("lofi");
        for _ in 0..20 {
            signals.observe_outcome(Outcome {
                id: "l",
                artist: "Chill",
                tags: &liked,
                reward: 1.0,
                context: &context,
            });
        }
        let disliked = tags("harsh noise");
        for _ in 0..20 {
            signals.observe_outcome(Outcome {
                id: "n",
                artist: "Wall",
                tags: &disliked,
                reward: 0.0,
                context: &context,
            });
        }
        assert!(signals.logistic(&liked, "Chill", &context) > 0.7);
        assert!(signals.logistic(&disliked, "Wall", &context) < 0.3);
        // Context is part of the model: the same genre is less likely in a
        // bucket that never produced a reward.
        let night = context_for(2, false);
        assert!(
            signals.logistic(&liked, "Chill", &night)
                < signals.logistic(&liked, "Chill", &context) - 0.05
        );
    }

    #[test]
    fn sequence_score_prefers_spread_artists_and_fresh_tracks() {
        let clumped: Vec<Track> = (0..3)
            .flat_map(|slot| {
                (0..4).map(move |index| {
                    track(
                        &format!("a{slot}{index}"),
                        "Song",
                        &format!("Artist {slot}"),
                        "phonk",
                    )
                })
            })
            .collect();
        let spread: Vec<Track> = (0..4)
            .flat_map(|index| {
                (0..3).map(move |slot| {
                    track(
                        &format!("b{slot}{index}"),
                        "Song",
                        &format!("Artist {slot}"),
                        "phonk",
                    )
                })
            })
            .collect();
        let fresh: HashSet<String> = HashSet::new();
        assert!(sequence_score(&spread, &[], &fresh) > sequence_score(&clumped, &[], &fresh));

        // Recently heard tracks should score badly at the front of the queue.
        let recent_track = spread[0].clone();
        let mut heard = HashSet::new();
        heard.insert(recent_track.id.clone());
        let mut with_recent_first = spread.clone();
        with_recent_first.rotate_left(1);
        with_recent_first.insert(0, recent_track);
        assert!(
            sequence_score(&with_recent_first, &[], &heard)
                < sequence_score(&spread, &[], &HashSet::new())
        );
    }

    #[test]
    fn shuffle_upcoming_keeps_the_prefix_and_the_track_set() {
        // Deliberately clumped: four tracks per artist in a row.
        let mut queue: Vec<Track> = (0..12)
            .map(|index| {
                track(
                    &format!("t{index}"),
                    "Song",
                    &format!("Artist {}", index / 4),
                    "phonk",
                )
            })
            .collect();
        let original: HashSet<String> = queue.iter().map(|track| track.id.clone()).collect();
        let prefix = queue[..3].to_vec();
        let before = sequence_score(&queue[3..], &[], &HashSet::new());
        shuffle_upcoming(&mut queue, 2, &[], 42);
        assert!(
            queue[..3]
                .iter()
                .zip(prefix.iter())
                .all(|(left, right)| left.id == right.id),
            "prefix must not move"
        );
        let after: HashSet<String> = queue[3..].iter().map(|track| track.id.clone()).collect();
        assert_eq!(
            after,
            original
                .difference(&prefix.iter().map(|t| t.id.clone()).collect())
                .cloned()
                .collect::<HashSet<_>>()
        );
        assert!(
            sequence_score(&queue[3..], &[], &HashSet::new()) >= before,
            "shuffle should not make the artist spacing worse"
        );
    }

    #[test]
    fn choose_next_skips_the_current_index() {
        let queue: Vec<Track> = (0..6)
            .map(|index| {
                track(
                    &format!("t{index}"),
                    "Song",
                    &format!("Artist {index}"),
                    "pop",
                )
            })
            .collect();
        for seed in 0..25 {
            let next = choose_next(&queue, 3, &[], seed).expect("a next index");
            assert_ne!(next, 3);
            assert!(next < queue.len());
        }
    }

    #[test]
    fn reward_curve_matches_implicit_feedback() {
        assert_eq!(reward_for(EventKind::Complete, 1.0), Some(1.0));
        assert_eq!(reward_for(EventKind::QuickSkip, 0.0), Some(0.0));
        assert_eq!(reward_for(EventKind::Error, 0.0), None);
        let half = reward_for(EventKind::Half, 0.5).unwrap();
        assert!((half - 0.51).abs() < 1e-9, "half reward {half}");
        assert!(reward_for(EventKind::SoftSkip, 0.9).unwrap() < 0.3);
    }

    #[test]
    fn directed_markov_transitions_record_natural_flow() {
        let mut signals = Signals::default();
        let first = track("t1", "Intro", "Artist", "ambient");
        let second = track("t2", "Outro", "Artist", "ambient");
        signals.observe_start(&first, &tags("ambient"));
        signals.observe_start(&second, &tags("ambient"));

        let context = context_for(14, false);
        signals.observe_outcome(Outcome {
            id: "t2",
            artist: "Artist",
            tags: &tags("ambient"),
            reward: 1.0,
            context: &context,
        });

        let prob = signals.markov_transition("t1", "t2");
        assert!(prob > 0.0, "markov transition probability must be > 0");
        assert_eq!(signals.markov_transition("t2", "t1"), 0.0);
    }

    #[test]
    fn skip_streak_triggers_recovery_mode() {
        let mut signals = Signals::default();
        let context = context_for(12, false);
        assert_eq!(signals.consecutive_skips, 0);

        // First quick skip
        signals.observe_outcome(Outcome {
            id: "s1",
            artist: "A",
            tags: &tags("pop"),
            reward: 0.0,
            context: &context,
        });
        assert_eq!(signals.consecutive_skips, 1);

        // Second quick skip enters recovery mode
        signals.observe_outcome(Outcome {
            id: "s2",
            artist: "B",
            tags: &tags("pop"),
            reward: 0.1,
            context: &context,
        });
        assert_eq!(signals.consecutive_skips, 2);

        // In recovery mode, high-affinity tracks get the anchor bonus
        let mut long_term = Affinity::default();
        long_term.tags.insert("ambient".to_string(), 2.0);
        let session = Session::default();
        let played_at = HashMap::new();
        let recent_artists = HashSet::new();
        let profile = Profile {
            long_term: &long_term,
            session: &session,
            signals: &signals,
            context: &context,
            played_at: &played_at,
            recent_artists: &recent_artists,
            recent: &[],
            last_primary: None,
            explore: 0.2,
        };

        let mut rng = Lcg::new(42);
        let candidate = track("c1", "Calm Song", "Zen", "ambient");
        let scored = score(&profile, &candidate, &tags("ambient"), &mut rng);
        assert!(
            scored.reason.contains("recovery anchor"),
            "expected recovery anchor in reason: {}",
            scored.reason
        );

        // Successful completion resets skip streak
        signals.observe_outcome(Outcome {
            id: "c1",
            artist: "Zen",
            tags: &tags("ambient"),
            reward: 1.0,
            context: &context,
        });
        assert_eq!(signals.consecutive_skips, 0);
    }

    #[test]
    fn energy_pacing_rewards_flow_and_penalizes_jarring_jumps() {
        let signals = Signals::default();
        let long_term = Affinity::default();
        let session = Session::default();
        let context = context_for(16, false);
        let played_at = HashMap::new();
        let recent_artists = HashSet::new();

        let ambient_prev = track("prev", "Quiet Ambient Drone", "Sleepy", "ambient");
        let recent_ambient = vec![ambient_prev];

        let profile_after_ambient = Profile {
            long_term: &long_term,
            session: &session,
            signals: &signals,
            context: &context,
            played_at: &played_at,
            recent_artists: &recent_artists,
            recent: &recent_ambient,
            last_primary: None,
            explore: 0.0,
        };

        let mut rng = Lcg::new(99);
        let metal_cand = track("m1", "Thrash Metal Chaos", "Rocker", "metal");
        let metal_tags = tags("metal");
        let metal_scored = score(&profile_after_ambient, &metal_cand, &metal_tags, &mut rng);
        assert!(
            metal_scored.pacing < 0.0,
            "jarring jump from ambient to metal should have negative pacing: {}",
            metal_scored.pacing
        );

        let lofi_cand = track("l1", "Chill Lofi Beats", "Chiller", "lofi");
        let lofi_tags = tags("lofi");
        let lofi_scored = score(&profile_after_ambient, &lofi_cand, &lofi_tags, &mut rng);
        assert!(
            lofi_scored.pacing > 0.0,
            "smooth flow from ambient to lofi should have positive pacing: {}",
            lofi_scored.pacing
        );
    }

    #[test]
    fn quality_filter_catches_all_slop_variations() {
        for title in [
            "Song (slowed & reverb)",
            "Song (slowed - reverb)",
            "Song (slowed / reverb)",
            "Song (slowed down)",
            "Song (sped-up)",
            "Song (8D Sound)",
            "Song (Daycore)",
            "Song (AI Vocal)",
            "Song (10 hour loop)",
            "Song (chopped & screwed)",
        ] {
            let candidate = track("x", title, "Someone", "pop");
            assert!(
                low_quality_reason(&candidate).is_some(),
                "{title} should be recognized as slop"
            );
        }
    }
}
