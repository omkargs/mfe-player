use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, Context, Result};
use image::imageops::FilterType;
use image::RgbImage;
use ratatui::style::Color;

use crate::theme::Theme;

/// Find the wallpaper currently in use and turn it into a palette.
///
/// `explicit` wins when it points at a real file (from `config.toml`), then
/// `MFE_WALLPAPER`, then whatever the compositor/desktop reports.
pub fn palette_for(explicit: Option<&str>) -> Result<(PathBuf, Theme)> {
    let path = explicit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(expand_tilde)
        .filter(|path| path.exists())
        .or_else(|| {
            env::var("MFE_WALLPAPER")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .map(expand_tilde)
                .filter(|path| path.exists())
        })
        .or_else(detect)
        .ok_or_else(|| {
            anyhow!(
                "no wallpaper found — set wallpaper = \"~/wall.png\" in the config \
                 or MFE_WALLPAPER=/path"
            )
        })?;
    let theme = palette(&path)?;
    Ok((path, theme))
}

pub fn palette(path: &Path) -> Result<Theme> {
    let image =
        image::open(path).with_context(|| format!("opening wallpaper {}", path.display()))?;
    Ok(palette_from_rgb(&image.to_rgb8()))
}

/// Build a palette from an image. Kept separate from file loading so it can be
/// tested without touching the disk.
pub fn palette_from_rgb(image: &RgbImage) -> Theme {
    let thumb = if image.width() > 64 || image.height() > 64 {
        image::imageops::resize(image, 64, 64, FilterType::Triangle)
    } else {
        image.clone()
    };

    let dominant = dominant_colour(&thumb);
    let (hue, saturation, _) = rgb_to_hsl(dominant[0], dominant[1], dominant[2]);
    let chroma = saturation.max(0.12);

    Theme {
        // Backgrounds keep the wallpaper's hue but go very dark. Panels stay
        // solid so text survives a bright wallpaper behind the app.
        bg: hsl(hue, chroma * 0.55, 0.055),
        panel: hsl(hue, chroma * 0.48, 0.085),
        panel2: hsl(hue, chroma * 0.42, 0.115),
        border: hsl(hue, chroma * 0.35, 0.24),
        fg: hsl(hue, chroma * 0.14, 0.90),
        dim: hsl(hue, chroma * 0.18, 0.62),
        faint: hsl(hue, chroma * 0.16, 0.44),
        accent: hsl(hue, chroma.max(0.55).min(0.95), 0.64),
        accent2: hsl((hue + 0.5) % 1.0, 0.72, 0.66),
        // Semantic colours stay predictable; people rely on green/yellow/red.
        green: Color::Rgb(126, 231, 135),
        yellow: Color::Rgb(255, 209, 102),
        red: Color::Rgb(255, 110, 110),
        purple: Color::Rgb(173, 142, 255),
    }
}

fn dominant_colour(image: &RgbImage) -> [u8; 3] {
    let pixels: Vec<[u8; 3]> = image.pixels().map(|pixel| pixel.0).collect();
    dominant_from_pixels(&pixels).unwrap_or([0, 0, 0])
}

/// Pick the most "identity" colour out of a list of RGB pixels: the bucket
/// that covers a lot of the image, is saturated enough to be a colour, and is
/// neither pure black nor pure white. Shared by the wallpaper palette and the
/// album-art ambient tint.
pub fn dominant_from_pixels(pixels: &[[u8; 3]]) -> Option<[u8; 3]> {
    // Quantise to 4 bits per channel and score buckets by how much of the
    // image they cover, how saturated they are, and whether they are mid-tone
    // (pure black/white corners are usually not the image's identity).
    let mut buckets: HashMap<u16, (u32, f32, f32, f32)> = HashMap::new();
    for pixel in pixels {
        let [r, g, b] = *pixel;
        let key = (((r >> 4) as u16) << 8) | (((g >> 4) as u16) << 4) | ((b >> 4) as u16);
        let entry = buckets.entry(key).or_insert((0, 0.0, 0.0, 0.0));
        entry.0 += 1;
        entry.1 += r as f32;
        entry.2 += g as f32;
        entry.3 += b as f32;
    }

    let mut best = (0.0f32, [0u8; 3]);
    for (count, r, g, b) in buckets.values() {
        let r = (r / *count as f32).round() as u8;
        let g = (g / *count as f32).round() as u8;
        let b = (b / *count as f32).round() as u8;
        let (_, saturation, luma) = rgb_to_hsl(r, g, b);
        if luma < 0.04 || luma > 0.96 {
            continue;
        }
        let mid_tone = 1.0 - (luma - 0.5).abs() * 1.2;
        let score = *count as f32 * (0.30 + saturation) * mid_tone.max(0.15);
        if score > best.0 {
            best = (score, [r, g, b]);
        }
    }

    if best.0 > 0.0 {
        return Some(best.1);
    }
    if pixels.is_empty() {
        return None;
    }
    let mut total = [0u64; 3];
    for pixel in pixels {
        for (index, channel) in pixel.iter().enumerate() {
            total[index] += *channel as u64;
        }
    }
    let count = pixels.len() as u64;
    Some([
        (total[0] / count) as u8,
        (total[1] / count) as u8,
        (total[2] / count) as u8,
    ])
}

/// Turn a dominant colour into a vivid accent pair, or `None` when the colour
/// is basically grey (a black-and-white cover should not tint the whole UI).
pub fn accent_pair(rgb: [u8; 3]) -> Option<(Color, Color)> {
    let (hue, saturation, _) = rgb_to_hsl(rgb[0], rgb[1], rgb[2]);
    if saturation < 0.10 {
        return None;
    }
    let accent = hsl(hue, saturation.clamp(0.55, 0.95), 0.64);
    let accent2 = hsl((hue + 0.5) % 1.0, 0.72, 0.66);
    Some((accent, accent2))
}

fn hsl(h: f32, s: f32, l: f32) -> Color {
    let (r, g, b) = hsl_to_rgb(h, s, l);
    Color::Rgb(r, g, b)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let h = h.rem_euclid(1.0);
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    if s <= f32::EPSILON {
        let v = (l * 255.0).round() as u8;
        return (v, v, v);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let channel = |t: f32| {
        let t = t.rem_euclid(1.0);
        let value = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        (value * 255.0).round().clamp(0.0, 255.0) as u8
    };
    (channel(h + 1.0 / 3.0), channel(h), channel(h - 1.0 / 3.0))
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let luma = (max + min) / 2.0;
    let delta = max - min;
    if delta <= f32::EPSILON {
        return (0.0, 0.0, luma);
    }
    let saturation = if luma > 0.5 {
        delta / (2.0 - max - min)
    } else {
        delta / (max + min)
    };
    let hue = if max == r {
        ((g - b) / delta + if g < b { 6.0 } else { 0.0 }) / 6.0
    } else if max == g {
        ((b - r) / delta + 2.0) / 6.0
    } else {
        ((r - g) / delta + 4.0) / 6.0
    };
    (hue, saturation.clamp(0.0, 1.0), luma)
}

/// Ask the desktop what wallpaper it is using. Best effort: every probe is
/// optional and failures are ignored.
pub fn detect() -> Option<PathBuf> {
    unix_desktop_detect()
        .or_else(macos_detect)
        .or_else(windows_detect)
        .map(expand_tilde)
        .filter(|path| path.exists())
}

fn unix_desktop_detect() -> Option<PathBuf> {
    // GNOME / Cinnamon / most gsettings desktops.
    if let Some(raw) = command_output(
        "gsettings",
        &["get", "org.gnome.desktop.background", "picture-uri"],
    ) {
        if let Some(path) = parse_file_uri(&raw) {
            return Some(path);
        }
    }

    // swww (Wayland wallpaper daemon): "currently displaying: image: /path".
    if let Some(raw) = command_output("swww", &["query"]) {
        if let Some(path) = raw
            .lines()
            .filter_map(|line| line.split("image:").nth(1))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .last()
        {
            return Some(PathBuf::from(path));
        }
    }

    // hyprpaper: "eDP-1 = /path" (newer) or "wallpaper = ..." (older).
    if let Some(raw) = command_output("hyprctl", &["hyprpaper", "listactive"]) {
        if let Some(path) = raw
            .lines()
            .filter_map(|line| line.split('=').nth(1))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .last()
        {
            return Some(PathBuf::from(path));
        }
    }

    // Plasma stores the image in the applets config.
    if let Ok(raw) =
        std::fs::read_to_string(home()?.join(".config/plasma-org.kde.plasma.desktop-appletsrc"))
    {
        if let Some(path) = raw
            .lines()
            .filter_map(|line| line.strip_prefix("Image="))
            .filter_map(parse_file_uri)
            .last()
        {
            return Some(path);
        }
    }

    None
}

fn macos_detect() -> Option<PathBuf> {
    let raw = command_output(
        "osascript",
        &[
            "-e",
            "tell application \"System Events\" to get picture of current desktop",
        ],
    )?;
    let path = raw.trim().trim_matches('"');
    (!path.is_empty()).then(|| PathBuf::from(path))
}

fn windows_detect() -> Option<PathBuf> {
    let raw = command_output(
        "reg",
        &["query", "HKCU\\Control Panel\\Desktop", "/v", "Wallpaper"],
    )?;
    raw.lines()
        .find_map(|line| line.split_whitespace().last())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn parse_file_uri(raw: &str) -> Option<PathBuf> {
    let trimmed = raw.trim().trim_matches('\'').trim_matches('"');
    if trimmed.is_empty() {
        return None;
    }
    let value = trimmed.strip_prefix("file://").unwrap_or(trimmed);
    Some(PathBuf::from(percent_decode(value)))
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = &input[index + 1..index + 3];
            if let Ok(value) = u8::from_str_radix(hex, 16) {
                out.push(value);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn expand_tilde(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    let raw = path.to_string_lossy();
    if raw == "~" {
        return home().unwrap_or_else(|| path.to_path_buf());
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = home() {
            return home.join(rest);
        }
    }
    path.to_path_buf()
}

fn home() -> Option<PathBuf> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    fn solid(colour: [u8; 3]) -> RgbImage {
        let mut image = RgbImage::new(16, 16);
        for pixel in image.pixels_mut() {
            *pixel = Rgb(colour);
        }
        image
    }

    #[test]
    fn dominant_from_pixels_ignores_black_borders() {
        let mut pixels = vec![[0, 0, 0]; 400];
        pixels.extend(vec![[220, 40, 60]; 100]);
        let dominant = dominant_from_pixels(&pixels).unwrap();
        assert_eq!(dominant, [220, 40, 60]);
    }

    #[test]
    fn accent_pair_is_vivid_and_ignores_grey() {
        let (accent, accent2) = accent_pair([220, 40, 60]).unwrap();
        assert!(matches!(accent, Color::Rgb(..)));
        assert_ne!(accent, accent2);
        assert!(accent_pair([128, 128, 128]).is_none());
    }

    #[test]
    fn derives_a_dark_tinted_theme_from_a_red_wallpaper() {
        let theme = palette_from_rgb(&solid([200, 40, 40]));
        let Color::Rgb(r, g, b) = theme.accent else {
            panic!("accent should be truecolor");
        };
        assert!(r > g && r > b, "accent should carry the wallpaper hue");
        let Color::Rgb(r, g, b) = theme.bg else {
            panic!("background should be truecolor");
        };
        assert!(r + g + b < 120, "background should stay dark");
        assert!(g < r || b < r);
    }

    #[test]
    fn a_grey_wallpaper_still_produces_readable_contrast() {
        let theme = palette_from_rgb(&solid([130, 130, 130]));
        let Color::Rgb(fr, fg, fb) = theme.fg else {
            panic!("foreground should be truecolor");
        };
        let Color::Rgb(br, bg, bb) = theme.bg else {
            panic!("background should be truecolor");
        };
        let fg_luma = fr as u32 + fg as u32 + fb as u32;
        let bg_luma = br as u32 + bg as u32 + bb as u32;
        assert!(fg_luma > bg_luma + 300, "text must be readable");
    }

    #[test]
    fn parses_file_uris() {
        assert_eq!(
            parse_file_uri("'file:///home/me/wall%20paper.png'").unwrap(),
            PathBuf::from("/home/me/wall paper.png")
        );
    }
}
