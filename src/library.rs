//! Local music library.
//!
//! Point `library_paths` at folders you actually own and MFE will scan them,
//! read tags with `lofty`, pull embedded cover art out of the file and make the
//! result playable without touching the network.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use lofty::prelude::*;

use crate::config;
use crate::models::Track;

const EXTENSIONS: &[&str] = &[
    "mp3", "flac", "opus", "ogg", "oga", "m4a", "aac", "wav", "aiff", "wv", "ape", "mpc",
];

fn hash(input: &str) -> String {
    let mut value: u64 = 0xcbf29ce484222325;
    for byte in input.as_bytes() {
        value ^= u64::from(*byte);
        value = value.wrapping_mul(0x100000001b3);
    }
    format!("{value:016x}")
}

fn expand(path: &str) -> PathBuf {
    let path = path.trim();
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

pub fn configured_paths(paths: &[String]) -> Vec<PathBuf> {
    paths
        .iter()
        .map(|path| expand(path))
        .filter(|path| path.is_dir())
        .collect()
}

fn collect(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 12 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with('.') {
                collect(&path, depth + 1, out);
            }
        } else if kind.is_file() {
            let supported = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
                .unwrap_or(false);
            if supported {
                out.push(path);
            }
        }
    }
}

fn embedded_art(path: &Path, tag: Option<&lofty::tag::Tag>) -> Option<String> {
    let picture = tag?.pictures().first()?;
    let bytes = picture.data();
    if bytes.is_empty() {
        return None;
    }
    let target = config::artwork_dir().join(format!(
        "embedded-{}.img",
        hash(&path.display().to_string())
    ));
    if let Some(parent) = target.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&target, bytes).ok()?;
    Some(target.display().to_string())
}

fn read_track(path: &Path) -> Option<Track> {
    let tagged = lofty::read_from_path(path).ok()?;
    let properties = tagged.properties();
    let duration = properties.duration().as_secs();
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());

    let stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown");
    let title = tag
        .and_then(|tag| tag.title())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| stem.trim().to_string());
    let artist = tag
        .and_then(|tag| tag.artist())
        .map(|value| value.trim().to_string())
        .unwrap_or_default();
    let album = tag
        .and_then(|tag| tag.album())
        .map(|value| value.trim().to_string())
        .unwrap_or_default();
    let year = tag
        .and_then(|tag| tag.get_string(ItemKey::Year))
        .map(|value| value.trim().to_string());
    let artwork = embedded_art(path, tag);

    Some(Track {
        id: format!("local:{}", path.display()),
        title,
        artist,
        album,
        duration: (duration > 0).then_some(duration),
        artwork,
        source: "local".to_string(),
        year,
    })
}

/// Walk every configured folder and turn files into playable tracks.
pub fn scan(paths: &[PathBuf]) -> Vec<Track> {
    let mut files = Vec::new();
    for path in paths {
        collect(path, 0, &mut files);
    }
    files.sort();
    files.dedup();
    let mut tracks: Vec<Track> = files.iter().filter_map(|path| read_track(path)).collect();
    tracks.sort_by(|a, b| {
        a.artist
            .to_ascii_lowercase()
            .cmp(&b.artist.to_ascii_lowercase())
            .then(
                a.album
                    .to_ascii_lowercase()
                    .cmp(&b.album.to_ascii_lowercase()),
            )
            .then(
                a.title
                    .to_ascii_lowercase()
                    .cmp(&b.title.to_ascii_lowercase()),
            )
    });
    tracks
}

pub fn load_cache() -> Vec<Track> {
    fs::read_to_string(config::library_cache_path())
        .ok()
        .and_then(|raw| serde_json::from_str::<Vec<Track>>(&raw).ok())
        .unwrap_or_default()
}

pub fn save_cache(tracks: &[Track]) -> Result<()> {
    let path = config::library_cache_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let raw = serde_json::to_string(tracks)?;
    fs::write(&path, raw).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smallest thing lofty will accept as a WAV: 44-byte header + silence.
    fn write_wav(path: &Path) {
        let samples: u32 = 800;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + samples * 2).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&8000u32.to_le_bytes());
        bytes.extend_from_slice(&16000u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&(samples * 2).to_le_bytes());
        bytes.extend(std::iter::repeat_n(0u8, (samples * 2) as usize));
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn scans_supported_files_and_skips_junk() {
        let dir = std::env::temp_dir().join(format!("mfe-library-test-{}", std::process::id()));
        let nested = dir.join("album");
        fs::create_dir_all(&nested).unwrap();
        write_wav(&nested.join("first track.wav"));
        fs::write(dir.join("cover.jpg"), b"not audio").unwrap();
        fs::write(dir.join("notes.txt"), b"hello").unwrap();

        let tracks = scan(&[dir.clone()]);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].title, "first track");
        assert_eq!(tracks[0].source, "local");
        assert!(tracks[0].id.starts_with("local:"));

        let _ = fs::remove_dir_all(dir);
    }

    /// Opt-in smoke test against a real collection:
    /// `MFE_TEST_LIBRARY=~/Music cargo test real_collection -- --ignored`
    #[test]
    #[ignore]
    fn real_collection() {
        let Ok(raw) = std::env::var("MFE_TEST_LIBRARY") else {
            return;
        };
        let paths = configured_paths(&[raw]);
        assert!(!paths.is_empty(), "no such folder");
        let tracks = scan(&paths);
        assert!(!tracks.is_empty(), "no audio files found");
        for track in tracks.iter().take(5) {
            println!("{} — {} ({})", track.artist, track.title, track.id);
        }
    }
}
