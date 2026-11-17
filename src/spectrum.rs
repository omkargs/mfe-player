//! In-process visualiser engine for the spectrum panel.
//!
//! `cava` handles audio capture because it already speaks PipeWire/PulseAudio
//! on Linux, CoreAudio (including the 14.2+ output tap) on macOS and WASAPI on
//! Windows. Everything that decides how the panel *looks* happens here: the
//! raw bars are remapped onto a mel-spaced grid, pushed through asymmetric
//! spring-damper physics, peak-held and scanned for beats. Rendering that
//! state at 60 fps is what makes the bars feel elastic instead of stepping
//! once per cava frame.
//!
//! The last snapshot is also mirrored into the runtime directory so
//! `mfe status --json` can hand a live mini-spectrum to waybar/sketchybar/YASB
//! without a second audio capture process.

use std::fs;
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Internal band count. High enough that a 100-column panel interpolates
/// smoothly, low enough that this stays a rounding error on the CPU.
pub const BANDS: usize = 64;

/// Frequency span cava analyses by default (`lower_cutoff_freq` /
/// `higher_cutoff_freq`). Bands are treated as reaching from F_MIN to F_MAX
/// and re-binned with the mel scale.
const F_MIN: f32 = 50.0;
const F_MAX: f32 = 12_000.0;

/// Springs may overshoot 1.0 a little; clamping at 1.25 keeps the tallest
/// bars from clipping while still allowing the springy 0.9→1.0 bounce.
const MAX_LEVEL: f32 = 1.25;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    /// Mel-spaced, smoothed bar levels (0..1.25).
    pub bands: Vec<f32>,
    /// Peak-hold caps for the same bands.
    pub peaks: Vec<f32>,
    /// Beat pulse: 1.0 on a detected transient, decaying to 0.
    pub pulse: f32,
    /// Overall smoothed loudness 0..1.
    pub energy: f32,
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
}

#[derive(Debug)]
pub struct Visualizer {
    bands: Vec<f32>,
    velocity: Vec<f32>,
    peaks: Vec<f32>,
    pulse: f32,
    energy: f32,
    bass: f32,
    mid: f32,
    treble: f32,
    /// Rolling average of the bass transient flux, the beat detector's
    /// adaptive noise floor.
    flux_avg: f32,
    last_bass: f32,
    beat_cooldown: f32,
}

impl Default for Visualizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Visualizer {
    pub fn new() -> Self {
        Self {
            bands: vec![0.0; BANDS],
            velocity: vec![0.0; BANDS],
            peaks: vec![0.0; BANDS],
            pulse: 0.0,
            energy: 0.0,
            bass: 0.0,
            mid: 0.0,
            treble: 0.0,
            flux_avg: 0.0,
            last_bass: 0.0,
            beat_cooldown: 0.0,
        }
    }

    /// Feed one frame of cava values. `dt` is seconds since the previous
    /// update, `active` is false while paused/idle so the bars fall to rest.
    pub fn update(&mut self, raw: &[f64], dt: f32, active: bool) {
        let dt = dt.clamp(0.001, 0.1);
        let target = if active && !raw.is_empty() {
            mel_bins(raw)
        } else {
            vec![0.0; BANDS]
        };

        for index in 0..BANDS {
            let target = target[index];
            let current = self.bands[index];
            // Asymmetric spring: attack quickly, release with damped inertia.
            // Using two sub-steps per frame prevents high-velocity overshoot
            // and makes the bars feel organic, bouncy and buttery smooth.
            let sub_dt = dt * 0.5;
            let mut pos = current;
            let mut vel = self.velocity[index];
            for _ in 0..2 {
                let stiffness = if target > pos { 340.0 } else { 130.0 };
                let damping = 24.0;
                let accel = stiffness * (target - pos) - damping * vel;
                vel = (vel + accel * sub_dt).clamp(-8.0, 8.0);
                pos = (pos + vel * sub_dt).clamp(0.0, MAX_LEVEL);
            }
            if target < 0.001 && pos < 0.001 {
                self.velocity[index] = 0.0;
                self.bands[index] = 0.0;
            } else {
                self.velocity[index] = vel;
                self.bands[index] = pos;
            }

            if self.bands[index] >= self.peaks[index] {
                self.peaks[index] = self.bands[index];
            } else {
                let fall = (dt * 0.42).max(self.peaks[index] * dt * 0.95);
                self.peaks[index] = (self.peaks[index] - fall).max(self.bands[index]);
            }
        }

        self.bass = self.band(0.0, 0.30);
        self.mid = self.band(0.30, 0.66);
        self.treble = self.band(0.66, 1.0);
        self.energy = (self.bass * 0.5 + self.mid * 0.32 + self.treble * 0.18).clamp(0.0, 1.0);

        self.update_beat(raw, dt, active);
    }

    fn band(&self, from: f32, to: f32) -> f32 {
        let start = (from * BANDS as f32) as usize;
        let end = ((to * BANDS as f32).ceil() as usize).clamp(start + 1, BANDS);
        let slice = &self.bands[start..end];
        if slice.is_empty() {
            return 0.0;
        }
        (slice.iter().sum::<f32>() / slice.len() as f32).clamp(0.0, 1.0)
    }

    fn update_beat(&mut self, raw: &[f64], dt: f32, active: bool) {
        if !active || raw.is_empty() {
            self.pulse = (self.pulse - dt * 3.0).max(0.0);
            self.flux_avg *= 0.90;
            self.last_bass = 0.0;
            self.beat_cooldown = (self.beat_cooldown - dt).max(0.0);
            return;
        }

        let take = ((raw.len() as f32 * 0.16).round() as usize).max(1);
        let bass_raw = (raw[..take].iter().sum::<f64>() / take as f64) as f32;
        let flux = (bass_raw - self.last_bass).max(0.0);
        self.last_bass = bass_raw;
        self.flux_avg = self.flux_avg * 0.90 + flux * 0.10;
        self.beat_cooldown = (self.beat_cooldown - dt).max(0.0);

        // A beat is a sharp rise above the recent transient average. The
        // cooldown keeps a single kick from machine-gunning the glow.
        let threshold = (self.flux_avg * 1.7).max(0.018);
        if self.beat_cooldown <= 0.0 && flux > threshold && bass_raw > 0.06 {
            self.pulse = 1.0;
            self.beat_cooldown = 0.16;
        }

        // Soft sub-bass sustain keeps the glow alive on tracks whose kicks
        // are compressed into a wall of sound.
        let sustain = ((bass_raw - 0.45) * 1.4).clamp(0.0, 0.45);
        if sustain > self.pulse {
            self.pulse = sustain;
        }
        self.pulse = (self.pulse - dt * 2.6).max(0.0);
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            bands: self.bands.clone(),
            peaks: self.peaks.clone(),
            pulse: self.pulse,
            energy: self.energy,
            bass: self.bass,
            mid: self.mid,
            treble: self.treble,
        }
    }

    pub fn pulse(&self) -> f32 {
        self.pulse
    }

    pub fn energy(&self) -> f32 {
        self.energy
    }

    pub fn bass(&self) -> f32 {
        self.bass
    }

    pub fn mid(&self) -> f32 {
        self.mid
    }

    pub fn treble(&self) -> f32 {
        self.treble
    }

    /// Interpolate the mel bands down to the panel's column count.
    pub fn levels(&self, columns: usize) -> Vec<f32> {
        interpolate(&self.bands, columns)
    }

    pub fn peak_line(&self, columns: usize) -> Vec<f32> {
        interpolate(&self.peaks, columns)
    }

    #[cfg(test)]
    pub fn bands(&self) -> &[f32] {
        &self.bands
    }
}

fn interpolate(values: &[f32], columns: usize) -> Vec<f32> {
    if columns == 0 || values.is_empty() {
        return Vec::new();
    }
    let last = values.len() - 1;
    (0..columns)
        .map(|column| {
            let position = if columns == 1 {
                0.0
            } else {
                column as f32 / (columns - 1) as f32 * last as f32
            };
            let low = position.floor() as usize;
            let high = (low + 1).min(last);
            let fraction = position - low as f32;
            values[low] * (1.0 - fraction) + values[high] * fraction
        })
        .collect()
}

fn mel(frequency: f32) -> f32 {
    2595.0 * (1.0 + frequency / 700.0).log10()
}

fn from_mel(mel_value: f32) -> f32 {
    700.0 * (10f32.powf(mel_value / 2595.0) - 1.0)
}

/// Re-bin linear cava bars onto a mel-spaced grid. The mel scale spends more
/// bands on the low end where music actually lives, which is why the result
/// looks like an analyser instead of 64 equal slices of FFT.
fn mel_bins(raw: &[f64]) -> Vec<f32> {
    let count = raw.len();
    let mut out = Vec::with_capacity(BANDS);
    if count == 0 {
        out.resize(BANDS, 0.0);
        return out;
    }
    let mel_min = mel(F_MIN);
    let mel_max = mel(F_MAX);
    for index in 0..BANDS {
        let fraction = index as f32 / (BANDS - 1) as f32;
        let frequency = from_mel(mel_min + (mel_max - mel_min) * fraction);
        let position = ((frequency - F_MIN) / (F_MAX - F_MIN)).clamp(0.0, 1.0) * (count - 1) as f32;
        let low = position.floor() as usize;
        let high = (low + 1).min(count - 1);
        let blend = position - low as f32;
        let value = raw[low] * (1.0 - blend) as f64 + raw[high] * blend as f64;
        // Mel bins undersample the treble; a small tilt keeps cymbals and
        // hi-hats visible instead of looking like dead columns.
        let tilt = 1.0 + 0.30 * fraction;
        out.push((value as f32 * tilt).clamp(0.0, MAX_LEVEL));
    }
    out
}

/// Last snapshot inside this process, for the daemon/UI status path.
static LATEST: RwLock<Option<Snapshot>> = RwLock::new(None);
static LAST_WRITE: Mutex<Option<Instant>> = Mutex::new(None);

/// Publish the current frame. `persist` writes the shared runtime file at most
/// every 150 ms so a status-bar poll never turns into a disk hammer.
pub fn publish(visualizer: &Visualizer, persist: bool) {
    let snapshot = visualizer.snapshot();
    if let Ok(mut latest) = LATEST.write() {
        *latest = Some(snapshot.clone());
    }
    if !persist {
        return;
    }
    let now = Instant::now();
    let Ok(mut last) = LAST_WRITE.lock() else {
        return;
    };
    let due = last
        .map(|at| now.duration_since(at) >= Duration::from_millis(150))
        .unwrap_or(true);
    if !due {
        return;
    }
    *last = Some(now);
    let path = crate::config::runtime_dir().join("spectrum.json");
    if let Ok(encoded) = serde_json::to_string(&snapshot) {
        let _ = fs::write(path, encoded);
    }
}

/// Snapshot shared inside this process (daemon `status_json` when it owns the
/// player). Cross-process callers should use [`read_published`].
pub fn latest() -> Option<Snapshot> {
    LATEST.read().ok().and_then(|snapshot| snapshot.clone())
}

/// Read the snapshot the player mirrored into the runtime directory. Stale
/// files (player exited without cleanup) are ignored.
pub fn read_published() -> Option<Snapshot> {
    let path = crate::config::runtime_dir().join("spectrum.json");
    let modified = fs::metadata(&path).ok()?.modified().ok()?;
    if modified.elapsed().ok()? > Duration::from_secs(3) {
        return None;
    }
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loud(count: usize, value: f64) -> Vec<f64> {
        vec![value; count]
    }

    #[test]
    fn mel_bins_favour_the_low_end() {
        // A tone only in the first raw bar should light more mel bands on the
        // left than a tone only in the last raw bar lights on the right.
        let mut low = vec![0.0; 28];
        low[0] = 1.0;
        let mut high = vec![0.0; 28];
        high[27] = 1.0;

        let low_bins = mel_bins(&low);
        let high_bins = mel_bins(&high);
        let low_lit = low_bins.iter().filter(|value| **value > 0.2).count();
        let high_lit = high_bins.iter().filter(|value| **value > 0.2).count();
        assert!(
            low_lit > high_lit,
            "mel grid should spend more bins on the bass ({low_lit} vs {high_lit})"
        );
    }

    #[test]
    fn silence_falls_to_rest() {
        let mut visualizer = Visualizer::new();
        for _ in 0..20 {
            visualizer.update(&loud(28, 0.9), 0.016, true);
        }
        assert!(visualizer.energy() > 0.1);
        for _ in 0..600 {
            visualizer.update(&[], 0.016, false);
        }
        assert!(visualizer.energy() < 0.01);
        assert!(visualizer.pulse() < 0.01);
        assert!(visualizer.bands().iter().all(|value| *value < 0.01));
    }

    #[test]
    fn bars_need_time_to_reach_a_constant_input() {
        let mut visualizer = Visualizer::new();
        visualizer.update(&loud(28, 0.8), 0.016, true);
        let first = visualizer.bands()[BANDS / 2];
        for _ in 0..10 {
            visualizer.update(&loud(28, 0.8), 0.016, true);
        }
        let later = visualizer.bands()[BANDS / 2];
        assert!(
            later > first,
            "spring should keep rising ({first} -> {later})"
        );
        assert!(later <= MAX_LEVEL);
    }

    #[test]
    fn a_kick_triggers_the_beat_pulse() {
        let mut visualizer = Visualizer::new();
        // Build a quiet floor so the adaptive threshold has a baseline.
        for _ in 0..30 {
            visualizer.update(&loud(28, 0.05), 0.016, true);
        }
        assert!(visualizer.pulse() < 0.2);
        visualizer.update(&loud(28, 1.0), 0.016, true);
        assert!(visualizer.pulse() > 0.5, "pulse was {}", visualizer.pulse());
        // ...and decays when the kick stops.
        for _ in 0..60 {
            visualizer.update(&loud(28, 0.1), 0.016, true);
        }
        assert!(visualizer.pulse() < 0.2);
    }

    #[test]
    fn levels_interpolate_to_any_width() {
        let mut visualizer = Visualizer::new();
        visualizer.update(&loud(28, 0.7), 0.016, true);
        for columns in [1, 7, 32, 100] {
            let levels = visualizer.levels(columns);
            assert_eq!(levels.len(), columns);
            assert!(levels.iter().all(|value| (0.0..=MAX_LEVEL).contains(value)));
        }
        assert!(visualizer.levels(0).is_empty());
    }
}
