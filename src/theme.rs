use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;

use ratatui::style::Color;

use crate::wallpaper;

/// A small, self-contained colour palette. The renderer reads it through
/// [`get`], so changing it in one place recolours the whole interface.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub bg: Color,
    pub panel: Color,
    pub panel2: Color,
    pub border: Color,
    pub fg: Color,
    pub dim: Color,
    pub faint: Color,
    pub accent: Color,
    pub accent2: Color,
    pub green: Color,
    pub yellow: Color,
    pub red: Color,
    pub purple: Color,
}

const MFE: Theme = Theme {
    bg: Color::Rgb(10, 12, 17),
    panel: Color::Rgb(16, 19, 27),
    panel2: Color::Rgb(21, 25, 36),
    border: Color::Rgb(49, 56, 74),
    fg: Color::Rgb(222, 226, 237),
    dim: Color::Rgb(112, 121, 143),
    faint: Color::Rgb(76, 84, 104),
    accent: Color::Rgb(255, 95, 148),
    accent2: Color::Rgb(92, 220, 238),
    green: Color::Rgb(126, 231, 135),
    yellow: Color::Rgb(255, 209, 102),
    red: Color::Rgb(255, 110, 110),
    purple: Color::Rgb(173, 142, 255),
};

const GRUVBOX: Theme = Theme {
    bg: Color::Rgb(29, 32, 33),
    panel: Color::Rgb(40, 40, 40),
    panel2: Color::Rgb(50, 48, 47),
    border: Color::Rgb(80, 73, 69),
    fg: Color::Rgb(235, 219, 178),
    dim: Color::Rgb(168, 153, 132),
    faint: Color::Rgb(124, 111, 100),
    accent: Color::Rgb(215, 153, 33),
    accent2: Color::Rgb(131, 165, 152),
    green: Color::Rgb(184, 187, 38),
    yellow: Color::Rgb(250, 189, 47),
    red: Color::Rgb(251, 73, 52),
    purple: Color::Rgb(211, 134, 155),
};

const NORD: Theme = Theme {
    bg: Color::Rgb(46, 52, 64),
    panel: Color::Rgb(59, 66, 82),
    panel2: Color::Rgb(67, 76, 94),
    border: Color::Rgb(76, 86, 106),
    fg: Color::Rgb(236, 239, 244),
    dim: Color::Rgb(216, 222, 233),
    faint: Color::Rgb(129, 161, 193),
    accent: Color::Rgb(136, 192, 208),
    accent2: Color::Rgb(143, 188, 187),
    green: Color::Rgb(163, 190, 140),
    yellow: Color::Rgb(235, 203, 139),
    red: Color::Rgb(191, 97, 106),
    purple: Color::Rgb(180, 142, 173),
};

const MONO: Theme = Theme {
    bg: Color::Rgb(8, 8, 8),
    panel: Color::Rgb(16, 16, 16),
    panel2: Color::Rgb(24, 24, 24),
    border: Color::Rgb(60, 60, 60),
    fg: Color::Rgb(235, 235, 235),
    dim: Color::Rgb(150, 150, 150),
    faint: Color::Rgb(95, 95, 95),
    accent: Color::Rgb(240, 240, 240),
    accent2: Color::Rgb(190, 190, 190),
    green: Color::Rgb(210, 210, 210),
    yellow: Color::Rgb(225, 225, 225),
    red: Color::Rgb(255, 255, 255),
    purple: Color::Rgb(175, 175, 175),
};

const MATRIX: Theme = Theme {
    bg: Color::Rgb(3, 10, 5),
    panel: Color::Rgb(6, 18, 9),
    panel2: Color::Rgb(8, 25, 12),
    border: Color::Rgb(25, 80, 38),
    fg: Color::Rgb(180, 255, 190),
    dim: Color::Rgb(80, 180, 100),
    faint: Color::Rgb(45, 110, 60),
    accent: Color::Rgb(90, 255, 110),
    accent2: Color::Rgb(140, 255, 160),
    green: Color::Rgb(120, 255, 130),
    yellow: Color::Rgb(200, 255, 140),
    red: Color::Rgb(255, 120, 120),
    purple: Color::Rgb(140, 220, 160),
};

const ROSE: Theme = Theme {
    bg: Color::Rgb(17, 11, 19),
    panel: Color::Rgb(26, 17, 29),
    panel2: Color::Rgb(34, 22, 38),
    border: Color::Rgb(74, 46, 80),
    fg: Color::Rgb(244, 226, 240),
    dim: Color::Rgb(170, 135, 168),
    faint: Color::Rgb(115, 88, 116),
    accent: Color::Rgb(255, 120, 180),
    accent2: Color::Rgb(200, 160, 255),
    green: Color::Rgb(150, 235, 180),
    yellow: Color::Rgb(255, 210, 150),
    red: Color::Rgb(255, 120, 140),
    purple: Color::Rgb(200, 150, 255),
};

const CATPPUCCIN: Theme = Theme {
    bg: Color::Rgb(30, 30, 46),
    panel: Color::Rgb(24, 24, 37),
    panel2: Color::Rgb(49, 50, 68),
    border: Color::Rgb(69, 71, 90),
    fg: Color::Rgb(205, 214, 244),
    dim: Color::Rgb(166, 173, 200),
    faint: Color::Rgb(108, 112, 134),
    accent: Color::Rgb(245, 194, 231),
    accent2: Color::Rgb(137, 220, 235),
    green: Color::Rgb(166, 227, 161),
    yellow: Color::Rgb(249, 226, 175),
    red: Color::Rgb(243, 139, 168),
    purple: Color::Rgb(203, 166, 247),
};

const DRACULA: Theme = Theme {
    bg: Color::Rgb(40, 42, 54),
    panel: Color::Rgb(33, 34, 44),
    panel2: Color::Rgb(68, 71, 90),
    border: Color::Rgb(98, 114, 164),
    fg: Color::Rgb(248, 248, 242),
    dim: Color::Rgb(189, 147, 249),
    faint: Color::Rgb(98, 114, 164),
    accent: Color::Rgb(255, 121, 198),
    accent2: Color::Rgb(139, 233, 253),
    green: Color::Rgb(80, 250, 123),
    yellow: Color::Rgb(241, 250, 140),
    red: Color::Rgb(255, 85, 85),
    purple: Color::Rgb(189, 147, 249),
};

const TOKYO_NIGHT: Theme = Theme {
    bg: Color::Rgb(26, 27, 38),
    panel: Color::Rgb(22, 22, 30),
    panel2: Color::Rgb(36, 40, 59),
    border: Color::Rgb(65, 72, 104),
    fg: Color::Rgb(192, 202, 245),
    dim: Color::Rgb(122, 162, 247),
    faint: Color::Rgb(86, 95, 137),
    accent: Color::Rgb(187, 154, 247),
    accent2: Color::Rgb(125, 207, 255),
    green: Color::Rgb(158, 206, 106),
    yellow: Color::Rgb(224, 175, 104),
    red: Color::Rgb(247, 118, 142),
    purple: Color::Rgb(187, 154, 247),
};

const SOLARIZED: Theme = Theme {
    bg: Color::Rgb(0, 43, 54),
    panel: Color::Rgb(7, 54, 66),
    panel2: Color::Rgb(88, 110, 117),
    border: Color::Rgb(101, 123, 131),
    fg: Color::Rgb(253, 246, 227),
    dim: Color::Rgb(147, 161, 161),
    faint: Color::Rgb(101, 123, 131),
    accent: Color::Rgb(38, 139, 210),
    accent2: Color::Rgb(42, 161, 152),
    green: Color::Rgb(133, 153, 0),
    yellow: Color::Rgb(181, 137, 0),
    red: Color::Rgb(220, 50, 47),
    purple: Color::Rgb(108, 113, 196),
};

const AMBER: Theme = Theme {
    bg: Color::Rgb(18, 12, 4),
    panel: Color::Rgb(28, 20, 7),
    panel2: Color::Rgb(38, 28, 10),
    border: Color::Rgb(92, 66, 20),
    fg: Color::Rgb(255, 219, 153),
    dim: Color::Rgb(196, 148, 66),
    faint: Color::Rgb(126, 92, 35),
    accent: Color::Rgb(255, 176, 0),
    accent2: Color::Rgb(255, 214, 102),
    green: Color::Rgb(186, 230, 126),
    yellow: Color::Rgb(255, 214, 102),
    red: Color::Rgb(255, 130, 92),
    purple: Color::Rgb(230, 168, 255),
};

const OCEAN: Theme = Theme {
    bg: Color::Rgb(8, 18, 30),
    panel: Color::Rgb(12, 26, 42),
    panel2: Color::Rgb(17, 36, 58),
    border: Color::Rgb(38, 70, 105),
    fg: Color::Rgb(210, 238, 255),
    dim: Color::Rgb(118, 167, 205),
    faint: Color::Rgb(72, 110, 145),
    accent: Color::Rgb(0, 200, 255),
    accent2: Color::Rgb(0, 255, 200),
    green: Color::Rgb(100, 235, 180),
    yellow: Color::Rgb(255, 220, 130),
    red: Color::Rgb(255, 120, 140),
    purple: Color::Rgb(150, 170, 255),
};

const ROSE_PINE: Theme = Theme {
    bg: Color::Rgb(25, 23, 36),
    panel: Color::Rgb(31, 29, 46),
    panel2: Color::Rgb(38, 35, 58),
    border: Color::Rgb(64, 61, 82),
    fg: Color::Rgb(224, 222, 244),
    dim: Color::Rgb(144, 140, 170),
    faint: Color::Rgb(110, 106, 134),
    accent: Color::Rgb(235, 111, 146),
    accent2: Color::Rgb(156, 207, 216),
    green: Color::Rgb(49, 116, 143),
    yellow: Color::Rgb(246, 193, 119),
    red: Color::Rgb(235, 111, 146),
    purple: Color::Rgb(196, 167, 231),
};

const KANAGAWA: Theme = Theme {
    bg: Color::Rgb(22, 22, 29),
    panel: Color::Rgb(31, 31, 40),
    panel2: Color::Rgb(42, 42, 55),
    border: Color::Rgb(84, 84, 109),
    fg: Color::Rgb(220, 215, 186),
    dim: Color::Rgb(114, 113, 105),
    faint: Color::Rgb(84, 84, 109),
    accent: Color::Rgb(126, 156, 216),
    accent2: Color::Rgb(127, 180, 202),
    green: Color::Rgb(152, 187, 108),
    yellow: Color::Rgb(230, 195, 132),
    red: Color::Rgb(232, 36, 36),
    purple: Color::Rgb(149, 127, 184),
};

/// The terminal's own palette. Every surface is `Reset`, so it inherits
/// whatever background and foreground the user already runs, and the accents
/// are the 16 ANSI colours rather than fixed RGB values.
const AUTO: Theme = Theme {
    bg: Color::Reset,
    panel: Color::Reset,
    panel2: Color::Reset,
    border: Color::DarkGray,
    fg: Color::Reset,
    dim: Color::Gray,
    faint: Color::DarkGray,
    accent: Color::Cyan,
    accent2: Color::Magenta,
    green: Color::Green,
    yellow: Color::Yellow,
    red: Color::Red,
    purple: Color::Magenta,
};

static CURRENT: RwLock<Theme> = RwLock::new(MFE);

/// The accent pair to restore when the ambient tint is disabled or a cover
/// has no usable colour. Only set while an ambient tint is active.
static AMBIENT_BASE: RwLock<Option<(Color, Color)>> = RwLock::new(None);

/// When enabled, the app stops painting the base background so the terminal's
/// own transparency (and therefore the wallpaper) shows through. Panels still
/// use `theme.panel`, so text stays readable.
static TRANSPARENT: AtomicBool = AtomicBool::new(false);

pub fn set_transparent(enabled: bool) {
    TRANSPARENT.store(enabled, Ordering::Relaxed);
}

pub fn transparent() -> bool {
    TRANSPARENT.load(Ordering::Relaxed)
}

/// Background for the app chrome. `Color::Reset` hands the cell back to the
/// terminal, which is what makes transparency work at all.
pub fn bg() -> Color {
    if transparent() {
        Color::Reset
    } else {
        get().bg
    }
}

/// Panel background. With transparency on this is also reset to the terminal,
/// so the wallpaper shows through the lists instead of only through the gaps.
pub fn panel() -> Color {
    if transparent() {
        Color::Reset
    } else {
        get().panel
    }
}

/// Secondary panel background (player bar, search field, hero cards).
pub fn panel2() -> Color {
    if transparent() {
        Color::Reset
    } else {
        get().panel2
    }
}

pub const THEME_NAMES: &[&str] = &[
    "mfe",
    "gruvbox",
    "nord",
    "mono",
    "matrix",
    "rose",
    "catppuccin",
    "dracula",
    "tokyo-night",
    "solarized",
    "amber",
    "ocean",
    "rose-pine",
    "kanagawa",
    "auto",
    "wallpaper",
];

fn preset(name: &str) -> Option<Theme> {
    match name.trim().to_ascii_lowercase().as_str() {
        "mfe" | "nuclear" | "pink" | "default" => Some(MFE),
        "gruvbox" | "warm" => Some(GRUVBOX),
        "nord" | "arctic" => Some(NORD),
        "mono" | "monochrome" | "gray" | "grey" => Some(MONO),
        "matrix" | "green" => Some(MATRIX),
        "rose" | "plum" => Some(ROSE),
        "catppuccin" | "mocha" | "catppuccin-mocha" => Some(CATPPUCCIN),
        "dracula" => Some(DRACULA),
        "tokyo-night" | "tokyonight" | "tokyo" => Some(TOKYO_NIGHT),
        "solarized" | "solarized-dark" | "solar" => Some(SOLARIZED),
        "amber" | "retro" | "terminal" => Some(AMBER),
        "ocean" | "deep" | "abyss" => Some(OCEAN),
        "rose-pine" | "rosepine" | "rosé-pine" => Some(ROSE_PINE),
        "kanagawa" | "kanagawa-wave" | "ink" => Some(KANAGAWA),
        "auto" | "ansi" | "terminal-palette" => Some(AUTO),
        // "wallpaper" is resolved from the desktop, not from a fixed palette.
        _ => None,
    }
}

/// Apply a theme by name. Unknown names keep the current palette and are
/// reported back as `false`.
pub fn set_theme(name: &str) -> bool {
    let Some(theme) = preset(name) else {
        return false;
    };
    if let Ok(mut current) = CURRENT.write() {
        *current = theme;
    }
    if let Ok(mut base) = AMBIENT_BASE.write() {
        *base = None;
    }
    true
}

/// How many colours the terminal can actually show. Resolved once from the
/// environment (override with `MFE_COLOR=truecolor|256|16`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum ColorMode {
    True,
    Ansi256,
    Ansi16,
}

fn color_mode() -> ColorMode {
    use std::sync::OnceLock;

    static MODE: OnceLock<ColorMode> = OnceLock::new();
    *MODE.get_or_init(|| {
        if let Ok(value) = std::env::var("MFE_COLOR") {
            match value.trim().to_ascii_lowercase().as_str() {
                "truecolor" | "24bit" | "true" => return ColorMode::True,
                "256" | "ansi256" => return ColorMode::Ansi256,
                "16" | "ansi" | "ansi16" => return ColorMode::Ansi16,
                _ => {}
            }
        }
        if let Ok(value) = std::env::var("COLORTERM") {
            if value.contains("truecolor") || value.contains("24bit") {
                return ColorMode::True;
            }
        }
        if std::env::var("WT_SESSION").is_ok() {
            return ColorMode::True;
        }
        if std::env::var("TERM")
            .map(|term| term.contains("256color") || term.contains("direct"))
            .unwrap_or(false)
        {
            return ColorMode::Ansi256;
        }
        if cfg!(windows) {
            ColorMode::Ansi256
        } else {
            ColorMode::Ansi16
        }
    })
}

/// Downgrade an RGB colour to the closest colour the terminal has. Named
/// colours and `Reset` are already portable and pass through untouched.
fn adapt(color: Color) -> Color {
    let Color::Rgb(r, g, b) = color else {
        return color;
    };
    match color_mode() {
        ColorMode::True => color,
        ColorMode::Ansi256 => Color::Indexed(nearest_256(r, g, b)),
        ColorMode::Ansi16 => Color::Indexed(nearest_16(r, g, b)),
    }
}

fn adapt_theme(theme: Theme) -> Theme {
    if color_mode() == ColorMode::True {
        return theme;
    }
    Theme {
        bg: adapt(theme.bg),
        panel: adapt(theme.panel),
        panel2: adapt(theme.panel2),
        border: adapt(theme.border),
        fg: adapt(theme.fg),
        dim: adapt(theme.dim),
        faint: adapt(theme.faint),
        accent: adapt(theme.accent),
        accent2: adapt(theme.accent2),
        green: adapt(theme.green),
        yellow: adapt(theme.yellow),
        red: adapt(theme.red),
        purple: adapt(theme.purple),
    }
}

fn distance(a: (u8, u8, u8), b: (u8, u8, u8)) -> u32 {
    let dr = i32::from(a.0) - i32::from(b.0);
    let dg = i32::from(a.1) - i32::from(b.1);
    let db = i32::from(a.2) - i32::from(b.2);
    (dr * dr + dg * dg + db * db) as u32
}

fn cube(value: u8) -> u8 {
    (((u16::from(value) * 5 + 127) / 255) as u8).min(5)
}

fn cube_level(step: u8) -> u8 {
    if step == 0 {
        0
    } else {
        55 + step * 40
    }
}

fn nearest_256(r: u8, g: u8, b: u8) -> u8 {
    let (ri, gi, bi) = (cube(r), cube(g), cube(b));
    let cube_index = 16 + 36 * ri + 6 * gi + bi;
    let cube_rgb = (cube_level(ri), cube_level(gi), cube_level(bi));
    let cube_distance = distance((r, g, b), cube_rgb);

    // The 24-step grey ramp (232-255) is much better for neutral colours.
    let grey = ((i32::from(r) + i32::from(g) + i32::from(b)) / 3).clamp(0, 255) as u8;
    let grey_step = (((i16::from(grey) - 8) / 10).clamp(0, 23)) as u8;
    let grey_level = 8 + grey_step * 10;
    let grey_index = 232 + grey_step;
    let grey_distance = distance((r, g, b), (grey_level, grey_level, grey_level));

    if grey_distance < cube_distance {
        grey_index
    } else {
        cube_index
    }
}

fn nearest_16(r: u8, g: u8, b: u8) -> u8 {
    const PALETTE: [(u8, u8, u8); 16] = [
        (0, 0, 0),
        (128, 0, 0),
        (0, 128, 0),
        (128, 128, 0),
        (0, 0, 128),
        (128, 0, 128),
        (0, 128, 128),
        (192, 192, 192),
        (128, 128, 128),
        (255, 0, 0),
        (0, 255, 0),
        (255, 255, 0),
        (0, 0, 255),
        (255, 0, 255),
        (0, 255, 255),
        (255, 255, 255),
    ];
    let mut best = (u32::MAX, 0u8);
    for (index, colour) in PALETTE.iter().enumerate() {
        let d = distance((r, g, b), *colour);
        if d < best.0 {
            best = (d, index as u8);
        }
    }
    best.1
}

pub fn get() -> Theme {
    let theme = CURRENT.read().map(|theme| *theme).unwrap_or(MFE);
    adapt_theme(theme)
}

/// Shift the accent colours to the dominant colour of the current cover.
///
/// `None` (or a grey cover) restores the exact accent pair the theme had
/// before the tint started, so the theme itself is never mutated.
pub fn apply_ambient(rgb: Option<[u8; 3]>) {
    let Ok(mut current) = CURRENT.write() else {
        return;
    };
    let Ok(mut saved) = AMBIENT_BASE.write() else {
        return;
    };
    match rgb.and_then(wallpaper::accent_pair) {
        Some((accent, accent2)) => {
            if saved.is_none() {
                *saved = Some((current.accent, current.accent2));
            }
            current.accent = accent;
            current.accent2 = accent2;
        }
        None => {
            if let Some((accent, accent2)) = saved.take() {
                current.accent = accent;
                current.accent2 = accent2;
            }
        }
    }
}

/// Map old aliases (including the pre-rename `nuclear` theme) onto the
/// canonical name shown in the UI.
pub fn canonical_name(name: &str) -> Option<&'static str> {
    let name = name.trim().to_ascii_lowercase();
    Some(match name.as_str() {
        "mfe" | "nuclear" | "pink" | "default" => "mfe",
        "gruvbox" | "warm" => "gruvbox",
        "nord" | "arctic" => "nord",
        "mono" | "monochrome" | "gray" | "grey" => "mono",
        "matrix" | "green" => "matrix",
        "rose" | "plum" => "rose",
        "catppuccin" | "mocha" | "catppuccin-mocha" => "catppuccin",
        "dracula" => "dracula",
        "tokyo-night" | "tokyonight" | "tokyo" => "tokyo-night",
        "solarized" | "solarized-dark" | "solar" => "solarized",
        "amber" | "retro" | "terminal" => "amber",
        "ocean" | "deep" | "abyss" => "ocean",
        "auto" | "ansi" | "terminal-palette" => "auto",
        "wallpaper" | "wall" | "desktop" => "wallpaper",
        _ => return None,
    })
}

/// Resolve a configured theme, with an optional explicit wallpaper path from
/// `config.toml`. `theme = "wallpaper"` derives the palette from the desktop.
pub fn apply_configured_with(name: &str, wallpaper_path: Option<&str>) -> String {
    if canonical_name(name) == Some("wallpaper") {
        return match adopt_wallpaper(wallpaper_path) {
            Ok(_) => "wallpaper".to_string(),
            Err(_) => {
                set_theme("mfe");
                "mfe".to_string()
            }
        };
    }
    match canonical_name(name) {
        Some(canonical) => {
            set_theme(canonical);
            canonical.to_string()
        }
        None => {
            set_theme("mfe");
            "mfe".to_string()
        }
    }
}

/// Derive a theme from the current wallpaper and make it active. Returns the
/// wallpaper path so the UI can tell the user which image it sampled.
pub fn adopt_wallpaper(explicit: Option<&str>) -> anyhow::Result<String> {
    let (path, theme) = wallpaper::palette_for(explicit)?;
    if let Ok(mut current) = CURRENT.write() {
        *current = theme;
    }
    if let Ok(mut base) = AMBIENT_BASE.write() {
        *base = None;
    }
    Ok(path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transparency_resets_every_background_layer() {
        set_transparent(true);
        assert_eq!(bg(), Color::Reset);
        assert_eq!(panel(), Color::Reset);
        assert_eq!(panel2(), Color::Reset);

        set_transparent(false);
        let theme = get();
        assert_eq!(bg(), theme.bg);
        assert_eq!(panel(), theme.panel);
        assert_eq!(panel2(), theme.panel2);
    }

    #[test]
    fn auto_theme_inherits_the_terminal_palette() {
        assert!(set_theme("auto"));
        let theme = get();
        assert_eq!(theme.bg, Color::Reset);
        assert_eq!(theme.panel, Color::Reset);
        assert_eq!(theme.fg, Color::Reset);
        assert_eq!(theme.accent, Color::Cyan);
        set_theme("mfe");
    }

    #[test]
    fn colour_fallbacks_stay_inside_their_palette() {
        assert_eq!(nearest_256(255, 0, 0), 196);
        assert_eq!(nearest_256(128, 128, 128), 244);
        assert!(nearest_16(12, 200, 180) < 16);
        assert_eq!(nearest_16(0, 0, 0), 0);
        assert_eq!(nearest_16(255, 255, 255), 15);
    }

    #[test]
    fn ambient_tint_restores_the_original_accents() {
        set_theme("mfe");
        let base = get();

        apply_ambient(Some([220, 40, 60]));
        assert_ne!(get().accent, base.accent);

        apply_ambient(None);
        assert_eq!(get().accent, base.accent);
        assert_eq!(get().accent2, base.accent2);

        // A grey cover leaves the theme alone instead of washing it out.
        apply_ambient(Some([128, 128, 128]));
        assert_eq!(get().accent, base.accent);
    }
}
