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

static CURRENT: RwLock<Theme> = RwLock::new(MFE);

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
    true
}

pub fn get() -> Theme {
    CURRENT.read().map(|theme| *theme).unwrap_or(MFE)
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
        "wallpaper" | "wall" | "desktop" => "wallpaper",
        _ => return None,
    })
}

/// Resolve a configured theme name, falling back to `mfe`.
pub fn apply_configured(name: &str) -> String {
    apply_configured_with(name, None)
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
    Ok(path.display().to_string())
}
