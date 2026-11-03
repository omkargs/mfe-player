//! Album art for the terminal.
//!
//! Cover URLs come from metadata plugins as ordinary JPEG/PNG/WebP files. We
//! cache the bytes, decode them with the `image` crate, and let the UI paint
//! them with truecolor half-blocks (`▀`) — no sixel, no Kitty protocol, works
//! in any modern terminal.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

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

fn fetch(url: &str, target: &PathBuf) -> Result<()> {
    if target.exists() && fs::metadata(target).map(|m| m.len() > 64).unwrap_or(false) {
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        let local = url.strip_prefix("file://").unwrap_or(url);
        if std::path::Path::new(local).exists() {
            fs::copy(local, target)?;
            return Ok(());
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
    Ok(())
}

/// Download (or reuse) and decode the artwork behind `url`.
pub fn load(url: &str) -> Result<Artwork> {
    let path = cached(url);
    fetch(url, &path)?;
    let bytes = fs::read(&path).context("reading cached artwork")?;
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
