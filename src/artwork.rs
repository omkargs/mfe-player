//! Album art for the terminal.
//!
//! Cover URLs come from metadata plugins as ordinary JPEG/PNG/WebP files. We
//! cache the bytes, decode them with the `image` crate, and let the UI paint
//! them with truecolor half-blocks (`▀`) — no sixel, no Kitty protocol, works
//! in any modern terminal.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use anyhow::{anyhow, Context, Result};

use crate::config;

#[derive(Debug, Clone)]
pub struct Artwork {
    pub width: usize,
    pub height: usize,
    /// Row-major RGB pixels, already downscaled to a sane size.
    pub pixels: Vec<[u8; 3]>,
}

impl Artwork {
    /// The cover's identity colour, used by the opt-in ambient tint.
    pub fn dominant(&self) -> Option<[u8; 3]> {
        crate::wallpaper::dominant_from_pixels(&self.pixels)
    }

    pub fn pixel(&self, x: usize, y: usize) -> [u8; 3] {
        let x = x.min(self.width.saturating_sub(1));
        let y = y.min(self.height.saturating_sub(1));
        self.pixels[y * self.width + x]
    }
}

fn hash(input: &str) -> String {
    let mut value: u64 = 0xcbf29ce484222325;
    for byte in input.as_bytes() {
        value ^= u64::from(*byte);
        value = value.wrapping_mul(0x100000001b3);
    }
    format!("{value:016x}")
}

fn cached(url: &str) -> PathBuf {
    config::artwork_dir().join(hash(url))
}

/// Covers are small, but a library can add up. Keep the cache useful instead
/// of letting it grow forever; override with MFE_ARTWORK_CACHE_MB.
const DEFAULT_CACHE_MB: u64 = 128;

pub fn cache_limit() -> u64 {
    std::env::var("MFE_ARTWORK_CACHE_MB")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_CACHE_MB)
        * 1024
        * 1024
}

fn touch(path: &Path) {
    if let Ok(file) = fs::OpenOptions::new().write(true).open(path) {
        let _ = file.set_modified(SystemTime::now());
    }
}

fn enforce_limit() {
    let _ = evict(&config::artwork_dir(), cache_limit());
}

/// Delete the oldest files until the directory is back under 90% of `limit`.
/// Returns the number of bytes freed. Best effort: a file that cannot be
/// removed is skipped rather than failing playback.
fn evict(dir: &Path, limit: u64) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let mut files: Vec<(SystemTime, u64, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let meta = entry.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            let age = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            Some((age, meta.len(), path))
        })
        .collect();

    let mut total: u64 = files.iter().map(|(_, size, _)| *size).sum();
    if total <= limit {
        return 0;
    }

    files.sort_by_key(|(age, _, _)| *age);
    let target = limit / 10 * 9;
    let mut freed = 0;
    for (_, size, path) in files {
        if total <= target {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(size);
            freed += size;
        }
    }
    freed
}

fn fetch(url: &str, target: &PathBuf) -> Result<bool> {
    if target.exists() && fs::metadata(target).map(|m| m.len() > 64).unwrap_or(false) {
        return Ok(false);
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        let local = url.strip_prefix("file://").unwrap_or(url);
        if std::path::Path::new(local).exists() {
            fs::copy(local, target)?;
            return Ok(true);
        }
        return Err(anyhow!("unsupported artwork source"));
    }
    let status = Command::new("curl")
        .arg("-fsSL")
        .arg("--max-time")
        .arg("12")
        .arg("--retry")
        .arg("1")
        .arg("-o")
        .arg(target)
        .arg(url)
        .status()
        .context("running curl for artwork")?;
    if !status.success() {
        return Err(anyhow!("artwork download failed"));
    }
    Ok(true)
}

/// Download (or reuse) and decode the artwork behind `url`.
pub fn load(url: &str) -> Result<Artwork> {
    let path = cached(url);
    if fetch(url, &path)? {
        enforce_limit();
    }
    let bytes = fs::read(&path).context("reading cached artwork")?;
    touch(&path);
    if bytes.len() < 64 {
        return Err(anyhow!("artwork file is empty"));
    }
    let image = image::load_from_memory(&bytes).context("decoding artwork")?;
    let image = if image.width() > 160 || image.height() > 160 {
        image.thumbnail(160, 160)
    } else {
        image
    }
    .to_rgb8();
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return Err(anyhow!("artwork has no pixels"));
    }
    let mut pixels = Vec::with_capacity((width * height) as usize);
    for pixel in image.pixels() {
        pixels.push([pixel[0], pixel[1], pixel[2]]);
    }
    Ok(Artwork {
        width: width as usize,
        height: height as usize,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn aged_file(path: &Path, bytes: usize, seconds_ago: u64) {
        fs::write(path, vec![7u8; bytes]).unwrap();
        let file = fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(SystemTime::now() - Duration::from_secs(seconds_ago))
            .unwrap();
    }

    #[test]
    fn evicts_the_least_recently_used_artwork_first() {
        let dir = std::env::temp_dir().join(format!("mfe-artwork-lru-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        aged_file(&dir.join("oldest"), 600, 300);
        aged_file(&dir.join("middle"), 600, 200);
        aged_file(&dir.join("newest"), 600, 100);

        let freed = evict(&dir, 1000);
        assert_eq!(freed, 1200, "should drop the two oldest files");
        assert!(!dir.join("oldest").exists());
        assert!(!dir.join("middle").exists());
        assert!(dir.join("newest").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn keeps_everything_under_the_limit() {
        let dir = std::env::temp_dir().join(format!("mfe-artwork-keep-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        aged_file(&dir.join("one"), 100, 20);
        assert_eq!(evict(&dir, 4096), 0);
        assert!(dir.join("one").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn decodes_a_local_cover() {
        let path = std::env::temp_dir().join("mfe-artwork-test.png");
        let image = image::RgbImage::from_fn(24, 16, |x, y| {
            image::Rgb([(x * 8) as u8, (y * 8) as u8, 120])
        });
        image.save(&path).unwrap();
        let art = load(path.to_str().unwrap()).expect("local artwork loads");
        assert_eq!((art.width, art.height), (24, 16));
        assert_eq!(art.pixels.len(), 24 * 16);
        let _ = std::fs::remove_file(path);
    }
}
