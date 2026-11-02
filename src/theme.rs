use std::sync::RwLock;

use ratatui::style::Color;

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

const NUCLEAR: Theme = Theme {
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

static CURRENT: RwLock<Theme> = RwLock::new(NUCLEAR);

pub const THEME_NAMES: &[&str] = &["nuclear", "gruvbox", "nord", "mono", "matrix", "rose"];

fn preset(name: &str) -> Option<Theme> {
    match name.trim().to_ascii_lowercase().as_str() {
        "nuclear" | "pink" | "default" => Some(NUCLEAR),
        "gruvbox" | "warm" => Some(GRUVBOX),
        "nord" | "arctic" => Some(NORD),
        "mono" | "monochrome" | "gray" | "grey" => Some(MONO),
        "matrix" | "green" => Some(MATRIX),
        "rose" | "plum" => Some(ROSE),
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
    CURRENT.read().map(|theme| *theme).unwrap_or(NUCLEAR)
}

/// Resolve a configured theme name, falling back to `nuclear`.
pub fn apply_configured(name: &str) -> String {
    if set_theme(name) {
        name.trim().to_ascii_lowercase()
    } else {
        set_theme("nuclear");
        "nuclear".to_string()
    }
}
